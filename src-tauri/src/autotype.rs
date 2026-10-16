//! Auto-type: escribe usuario, Tab, contraseña… en la ventana activa con `SendInput` (Unicode, así que da igual
//! la distribución del teclado).

use std::time::Duration;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VK_CONTROL,
    VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN, VK_SHIFT, VK_TAB,
};
use zeroize::Zeroizing;

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Text(String),
    /// Clave de plantilla (`username`, `password`…) o etiqueta de un campo personalizado
    Field(String),
    Totp,
    Tab,
    Enter,
    Delay(u64),
}

pub fn parse(seq: &str) -> Vec<Token> {
    let mut out = vec![];
    let mut text = String::new();
    let mut rest = seq;
    while let Some(start) = rest.find('{') {
        text.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            text.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let inner = &after[..end];
        let upper = inner.trim().to_uppercase();
        let token = match upper.as_str() {
            "USERNAME" | "USUARIO" => Some(Token::Field("username".into())),
            "PASSWORD" | "CONTRASEÑA" => Some(Token::Field("password".into())),
            "TOTP" | "2FA" => Some(Token::Totp),
            "TAB" => Some(Token::Tab),
            "ENTER" => Some(Token::Enter),
            _ if upper.starts_with("DELAY ") => upper[6..].trim().parse().ok().map(|ms: u64| Token::Delay(ms.min(10_000))),
            _ => inner.split_once(':').filter(|(k, _)| matches!(k.trim().to_lowercase().as_str(), "campo" | "field")).map(|(_, v)| Token::Field(v.trim().into())),
        };
        match token {
            Some(t) => {
                if !text.is_empty() {
                    out.push(Token::Text(std::mem::take(&mut text)));
                }
                out.push(t);
            }
            None => text.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    text.push_str(rest);
    if !text.is_empty() {
        out.push(Token::Text(text));
    }
    out
}

pub fn host_of(url: &str) -> Option<String> {
    let h = url.trim().split("://").last()?.split(['/', '?', '#']).next()?;
    // Sin puerto ni usuario (`user@host:8080`)
    let h = h.rsplit('@').next()?.split(':').next()?.trim_end_matches('.').to_lowercase();
    (!h.is_empty()).then_some(h)
}

/// Dominio registrable según la Public Suffix List: `https://accounts.google.co.uk/x` → `google.co.uk`,
/// `www.ing.es` → `ing.es`, `a.github.io` → `a.github.io` (cada página de github.io es de alguien distinto).
/// Una IP o un nombre sin punto se comparan tal cual.
pub fn registrable(url: &str) -> Option<String> {
    let host = host_of(url)?;
    if host.parse::<std::net::IpAddr>().is_ok() || !host.contains('.') {
        return Some(host);
    }
    psl::domain_str(&host).map(str::to_string)
}

/// Nombre reconocible de una web para buscarlo en el título de una ventana: `www.netflix.com` → `netflix`.
pub fn site_name(url: &str) -> Option<String> {
    let reg = registrable(url)?;
    let core = reg.split('.').next()?;
    (core.len() >= 3).then(|| core.to_string())
}

/// Entradas cuya web es del mismo dominio que la página abierta (la URL real del navegador).
pub fn matches_url<'a>(page: &str, entries: impl Iterator<Item = (&'a str, Vec<String>)>) -> Vec<String> {
    let Some(target) = registrable(page) else { return vec![] };
    entries.filter(|(_, urls)| urls.iter().any(|u| registrable(u).as_deref() == Some(&target))).map(|(id, _)| id.to_string()).collect()
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// ¿Aparecen esas palabras seguidas y completas? («ing» no casa con «shopping»)
fn has_words(hay: &[String], needle: &[String]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Entradas (id, título, webs) que encajan con el título de la ventana, por palabras completas. Es una pista,
/// no una prueba: con esto nunca se escribe sin que la persona confirme.
pub fn matches<'a>(window: &str, entries: impl Iterator<Item = (&'a str, &'a str, Vec<String>)>) -> Vec<String> {
    // El nombre del navegador no cuenta («… - Google Chrome» casaría con cualquier cuenta de Google)
    let mut w = window.to_lowercase();
    for b in ["google chrome", "mozilla firefox", "microsoft edge", "microsoft\u{200b} edge", "brave", "opera", "vivaldi"] {
        if let Some(i) = w.rfind(b) {
            w.truncate(i);
        }
    }
    let hay = words(&w);
    entries
        .filter(|(_, title, urls)| {
            let t = words(title);
            (t.join(" ").chars().count() >= 3 && has_words(&hay, &t)) || urls.iter().filter_map(|u| site_name(u)).any(|s| has_words(&hay, &[s]))
        })
        .map(|(id, _, _)| id.to_string())
        .collect()
}

/// Lo que se va a escribir, ya resuelto (los secretos en claro solo aquí y se borran al terminar).
pub enum Action {
    Text(Zeroizing<String>),
    Key(u16),
    Delay(u64),
}

fn key_input(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn send(inputs: &[INPUT]) {
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
}

/// Espera a que sueltes Ctrl/Alt/Mayús/Win del atajo (si no, «a» saldría como Ctrl+Alt+a).
pub fn wait_modifiers_released() {
    for _ in 0..100 {
        let held = [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN].iter().any(|vk| unsafe { GetAsyncKeyState(*vk as i32) } as u16 & 0x8000 != 0);
        if !held {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn perform(actions: &[Action]) {
    wait_modifiers_released();
    std::thread::sleep(Duration::from_millis(60));
    for a in actions {
        match a {
            Action::Text(t) => {
                for u in t.encode_utf16() {
                    send(&[key_input(0, u, KEYEVENTF_UNICODE), key_input(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)]);
                    std::thread::sleep(Duration::from_millis(4));
                }
            }
            Action::Key(vk) => {
                send(&[key_input(*vk, 0, 0), key_input(*vk, 0, KEYEVENTF_KEYUP)]);
                std::thread::sleep(Duration::from_millis(30));
            }
            Action::Delay(ms) => std::thread::sleep(Duration::from_millis(*ms)),
        }
    }
}

pub const TAB: u16 = VK_TAB;
pub const ENTER: u16 = VK_RETURN;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sequences() {
        use Token::*;
        assert_eq!(parse("{USERNAME}{TAB}{PASSWORD}{ENTER}"), vec![Field("username".into()), Tab, Field("password".into()), Enter]);
        assert_eq!(
            parse("hola {delay 500}{campo:PIN}{TOTP} {raro} {abierto"),
            vec![Text("hola ".into()), Delay(500), Field("PIN".into()), Totp, Text(" {raro} {abierto".into())]
        );
        assert_eq!(parse("{DELAY 99999}"), vec![Delay(10_000)]);
        assert_eq!(parse(""), vec![]);
    }

    #[test]
    fn matches_by_real_url() {
        assert_eq!(registrable("https://accounts.google.co.uk/signin").as_deref(), Some("google.co.uk"));
        assert_eq!(registrable("www.netflix.com/es/login").as_deref(), Some("netflix.com"));
        assert_eq!(registrable("localhost").as_deref(), Some("localhost"));
        assert_eq!(registrable("www.ing.es").as_deref(), Some("ing.es"));
        assert_eq!(registrable("https://ing.es/login").as_deref(), Some("ing.es"));
        assert_ne!(registrable("a.github.io"), registrable("b.github.io"));
        assert_ne!(registrable("x.vercel.app"), registrable("y.vercel.app"));
        assert_eq!(registrable("http://192.168.1.1:8080/admin").as_deref(), Some("192.168.1.1"));
        assert_eq!(registrable("https://user@netflix.com:443/").as_deref(), Some("netflix.com"));
        let entries = [
            ("1", vec!["https://www.netflix.com".to_string()]),
            ("2", vec!["https://mail.google.com".to_string()]),
            ("3", vec!["https://netflix.com.evil.io".to_string()]),
            ("4", vec![]),
        ];
        let m = |p: &str| matches_url(p, entries.iter().map(|(a, b)| (*a, b.clone())));
        assert_eq!(m("netflix.com/browse"), vec!["1"]);
        assert_eq!(m("https://accounts.google.com/v3/signin"), vec!["2"]);
        assert!(m("https://evil.io").len() == 1 && m("https://evil.io")[0] == "3");
        assert!(m("nada").is_empty());
    }

    #[test]
    fn matches_window_titles() {
        assert_eq!(site_name("https://www.netflix.com/login").as_deref(), Some("netflix"));
        assert_eq!(site_name("accounts.google.co.uk").as_deref(), Some("google"));
        assert_eq!(site_name("http://localhost:3000").as_deref(), Some("localhost"));
        assert_eq!(site_name("www.ing.es").as_deref(), Some("ing"));
        assert_eq!(site_name("x.io"), None);
        let entries = [
            ("1", "Netflix", vec!["https://www.netflix.com".to_string()]),
            ("2", "Banco", vec!["https://bancosantander.es".to_string()]),
            ("3", "Mi correo", vec!["https://mail.google.com".to_string()]),
            ("4", "TV", vec![]),
        ];
        let m = |t: &str| matches(t, entries.iter().map(|(a, b, c)| (*a, *b, c.clone())));
        assert_eq!(m("Netflix - Google Chrome"), vec!["1"]);
        assert!(m("Bandeja de entrada - Gmail - Google Chrome").is_empty());
        assert_eq!(m("Google - Mozilla Firefox"), vec!["3"]);
        assert_eq!(m("Inicio de sesión | Bancosantander"), vec!["2"]);
        assert!(m("Bloc de notas").is_empty());
        // Palabras completas: «ING» no casa con «Shopping»
        let ing = [("9", "ING", Vec::<String>::new())];
        let mi = |t: &str| matches(t, ing.iter().map(|(a, b, c)| (*a, *b, c.clone())));
        assert!(mi("Shopping - Firefox").is_empty());
        assert_eq!(mi("ING Direct - Banca online"), vec!["9"]);
    }
}
