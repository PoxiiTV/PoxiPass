//! Puente con la extensión del navegador. El relé (`relay.rs`) pasa aquí, por una tubería con nombre, lo que la
//! extensión manda por Native Messaging: tramas de 4 bytes de longitud + JSON. Solo se habla con nuestro mismo exe
//! y con navegadores emparejados; los secretos salen de uno en uno, siempre para el dominio de la página que los pide.

use crate::autotype::{host_of, registrable};
use crate::commands::{self, check_len, mutate, with, AppState};
use crate::error::{AppError, Res};
use crate::session::{Session, MAX_HISTORY};
use crate::util::now;
use crate::vault::model::*;
use crate::{clipboard, config, generator, hello, totp};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::{mpsc, oneshot};

pub const HOST_NAME: &str = "com.poxi.poxipass";
/// IDs de la extensión que pueden lanzar el relé: el de desarrollo (fijado con `key` en su manifest) y, cuando se
/// publique, los que asignen Chrome Web Store y Edge Add-ons.
const EXTENSION_IDS: [&str; 1] = ["hmaoebkafmmaajebmhanapkmapfalhpb"];
/// Brave, Opera y Vivaldi leen la clave de Chrome
const REG_KEYS: [&str; 3] = [r"Software\Google\Chrome\NativeMessagingHosts", r"Software\Chromium\NativeMessagingHosts", r"Software\Microsoft\Edge\NativeMessagingHosts"];
const MAX_FRAME: usize = 64 * 1024;
const PAIR_TIMEOUT: Duration = Duration::from_secs(120);

// ————————————————————————————————————————————————————————————— tubería y procesos

/// Una tubería por usuario y sesión de Windows (con dos sesiones abiertas, cada una habla con su app).
pub fn pipe_name() -> String {
    let mut session = 0u32;
    unsafe { windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(std::process::id(), &mut session) };
    let user: String = std::env::var("USERNAME").unwrap_or_default().chars().filter(char::is_ascii_alphanumeric).collect();
    format!(r"\\.\pipe\poxipass-bridge-{}-{session}", user.to_lowercase())
}

fn raw(h: &impl std::os::windows::io::AsRawHandle) -> windows_sys::Win32::Foundation::HANDLE {
    h.as_raw_handle() as _
}

pub fn server_pid(p: &tokio::net::windows::named_pipe::NamedPipeClient) -> Option<u32> {
    let mut pid = 0;
    (unsafe { windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(raw(p), &mut pid) } != 0).then_some(pid)
}

fn client_pid(p: &NamedPipeServer) -> Option<u32> {
    let mut pid = 0;
    (unsafe { windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId(raw(p), &mut pid) } != 0).then_some(pid)
}

fn exe_of(pid: u32) -> Option<PathBuf> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

/// ¿Ese proceso es este mismo PoxiPass.exe? (el relé y la app comprueban al otro lado de la tubería)
pub fn is_our_exe(pid: u32) -> bool {
    let canon = |p: PathBuf| std::fs::canonicalize(p).ok();
    match (exe_of(pid).and_then(canon), std::env::current_exe().ok().and_then(canon)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

// ————————————————————————————————————————————————————————————— registro en el navegador

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn reg_set(sub: &str, value: &str) {
    use windows_sys::Win32::System::Registry::*;
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let opts = REG_OPTION_NON_VOLATILE;
        if RegCreateKeyExW(HKEY_CURRENT_USER, wide(sub).as_ptr(), 0, std::ptr::null(), opts, KEY_WRITE, std::ptr::null(), &mut key, std::ptr::null_mut()) != 0 {
            return;
        }
        let v = wide(value);
        RegSetValueExW(key, std::ptr::null(), 0, REG_SZ, v.as_ptr().cast(), (v.len() * 2) as u32);
        RegCloseKey(key);
    }
}

fn reg_delete(sub: &str) {
    unsafe { windows_sys::Win32::System::Registry::RegDeleteKeyW(windows_sys::Win32::System::Registry::HKEY_CURRENT_USER, wide(sub).as_ptr()) };
}

/// Da de alta (o de baja) el relé en los navegadores. Apunta al exe actual: vale para el portable y se repara solo
/// si se mueve. Escribir en HKCU no pide permisos y no hace nada mientras no haya extensión.
pub fn register(data_dir: &Path, enabled: bool) {
    let manifest = data_dir.join("native-host.json");
    if !enabled {
        for k in REG_KEYS {
            reg_delete(&format!(r"{k}\{HOST_NAME}"));
        }
        let _ = std::fs::remove_file(manifest);
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let json = json!({
        "name": HOST_NAME,
        "description": "PoxiPass",
        "path": exe,
        "type": "stdio",
        "allowed_origins": EXTENSION_IDS.iter().map(|id| format!("chrome-extension://{id}/")).collect::<Vec<_>>(),
    });
    let _ = std::fs::create_dir_all(data_dir);
    if std::fs::write(&manifest, serde_json::to_vec_pretty(&json).unwrap_or_default()).is_ok() {
        for k in REG_KEYS {
            reg_set(&format!(r"{k}\{HOST_NAME}"), &manifest.to_string_lossy());
        }
    }
}

// ————————————————————————————————————————————————————————————— instalar la extensión

/// IDs en las tiendas: vacíos hasta publicarla (y entonces también van a `EXTENSION_IDS`).
const CHROME_STORE_ID: &str = "";
const EDGE_STORE_ID: &str = "";

/// Navegadores compatibles: clave, nombre, exe en App Paths y ruta típica en %LOCALAPPDATA% si no se registra ahí.
/// Todos instalan de la tienda de Chrome menos Edge.
const BROWSERS: [(&str, &str, &str, &str); 5] = [
    ("chrome", "Google Chrome", "chrome.exe", r"Google\Chrome\Application\chrome.exe"),
    ("edge", "Microsoft Edge", "msedge.exe", ""),
    ("brave", "Brave", "brave.exe", r"BraveSoftware\Brave-Browser\Application\brave.exe"),
    ("opera", "Opera", "opera.exe", r"Programs\Opera\opera.exe"),
    ("vivaldi", "Vivaldi", "vivaldi.exe", r"Vivaldi\Application\vivaldi.exe"),
];

fn store_url(key: &str) -> Option<String> {
    let (id, store) = if key == "edge" { (EDGE_STORE_ID, "https://microsoftedge.microsoft.com/addons") } else { (CHROME_STORE_ID, "https://chromewebstore.google.com") };
    if !id.is_empty() {
        return Some(format!("{store}/detail/{id}"));
    }
    // Sin publicar todavía: en desarrollo, la búsqueda de la tienda deja probar el botón
    cfg!(debug_assertions).then(|| format!("{store}/search/PoxiPass"))
}

/// Ruta del navegador: la que registra su instalador en App Paths (para el usuario o para todos) o la típica.
fn browser_exe(exe: &str, local: &str) -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::*;
    let sub = wide(&format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{exe}"));
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let mut buf = [0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        if unsafe { RegGetValueW(root, sub.as_ptr(), std::ptr::null(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len) } == 0 {
            let p = PathBuf::from(String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]).trim_matches('"'));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let p = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(local);
    (!local.is_empty() && p.is_file()).then_some(p)
}

#[derive(Serialize)]
pub struct InstallTarget {
    key: &'static str,
    name: &'static str,
}

/// Navegadores instalados en los que se puede instalar la extensión (ninguno hasta que esté en las tiendas)
#[tauri::command]
pub fn extension_targets() -> Vec<InstallTarget> {
    BROWSERS.iter().filter(|b| store_url(b.0).is_some() && browser_exe(b.2, b.3).is_some()).map(|b| InstallTarget { key: b.0, name: b.1 }).collect()
}

/// Abre la página de la extensión en la tienda, en ese navegador (la URL nunca viene de la interfaz)
#[tauri::command]
pub fn extension_install(key: String) -> Res<()> {
    let b = BROWSERS.iter().find(|b| b.0 == key).ok_or(AppError::new("io.notFound"))?;
    let (Some(url), Some(exe)) = (store_url(b.0), browser_exe(b.2, b.3)) else { return Err(AppError::new("io.notFound")) };
    std::process::Command::new(exe).arg(url).spawn()?;
    Ok(())
}

// ————————————————————————————————————————————————————————————— navegadores emparejados

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Paired {
    pub id: String,
    pub name: String,
    /// SHA-256 de la clave (la clave solo la tiene la extensión)
    #[serde(skip_serializing_if = "String::is_empty", default)]
    hash: String,
    pub created: i64,
    pub used: i64,
}

static PAIRS: Mutex<Option<Vec<Paired>>> = Mutex::new(None);
static PENDING_PAIR: Mutex<Option<oneshot::Sender<bool>>> = Mutex::new(None);

fn pairs(dir: &Path) -> MutexGuard<'static, Option<Vec<Paired>>> {
    let mut g = PAIRS.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(std::fs::read(dir.join("browsers.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default());
    }
    g
}

fn save_pairs(dir: &Path, list: &[Paired]) {
    if let Ok(json) = serde_json::to_vec_pretty(list) {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join("browsers.json"), json);
    }
}

fn hash_key(key: &str) -> String {
    Sha256::digest(key.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn verify_pair(dir: &Path, id: &str, key: &str) -> bool {
    let mut g = pairs(dir);
    let list = g.get_or_insert_with(Vec::new);
    let h = hash_key(key);
    let Some(p) = list.iter_mut().find(|p| p.id == id && ct_eq(p.hash.as_bytes(), h.as_bytes())) else { return false };
    p.used = now();
    save_pairs(dir, list);
    true
}

fn new_pair(dir: &Path, name: &str) -> (String, String) {
    let mut bytes = [0u8; 32];
    let _ = getrandom::fill(&mut bytes);
    let key = B64.encode(bytes);
    let id = new_id();
    let mut g = pairs(dir);
    let list = g.get_or_insert_with(Vec::new);
    list.push(Paired { id: id.clone(), name: name.to_string(), hash: hash_key(&key), created: now(), used: now() });
    save_pairs(dir, list);
    (id, key)
}

#[tauri::command]
pub fn browsers_list(st: tauri::State<'_, AppState>) -> Vec<Paired> {
    pairs(&st.data_dir).clone().unwrap_or_default().into_iter().map(|p| Paired { hash: String::new(), ..p }).collect()
}

#[tauri::command]
pub fn browser_forget(st: tauri::State<'_, AppState>, id: String) -> Vec<Paired> {
    {
        let mut g = pairs(&st.data_dir);
        let list = g.get_or_insert_with(Vec::new);
        list.retain(|p| p.id != id);
        save_pairs(&st.data_dir, list);
    }
    browsers_list(st)
}

/// Respuesta de la persona al «¿Conectar este navegador?»
#[tauri::command]
pub fn bridge_pair_answer(ok: bool) {
    if let Some(tx) = PENDING_PAIR.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = tx.send(ok);
    }
}

// ————————————————————————————————————————————————————————————— lógica (sin E/S, con pruebas)

fn live(e: &Entry) -> bool {
    e.deleted.is_none() && !e.archived
}

fn urls(e: &Entry) -> impl Iterator<Item = &str> {
    e.fields.iter().filter(|f| f.kind == FieldKind::Url).map(|f| f.value.plain_str())
}

/// ¿La web de la entrada es del mismo dominio registrable que la página? `Some(true)` si además es el mismo host.
fn site_match(e: &Entry, page: &str) -> Option<bool> {
    let (target, host) = (registrable(page)?, host_of(page)?);
    urls(e).filter(|u| registrable(u).as_deref() == Some(target.as_str())).map(|u| host_of(u).as_deref() == Some(host.as_str())).reduce(|a, b| a || b)
}

fn text(s: &Session, e: &Entry, key: &str) -> String {
    e.field(key).map(|f| f.value.reveal_string(&s.sealer).to_string()).unwrap_or_default()
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LoginView {
    pub id: String,
    pub title: String,
    pub username: String,
    pub totp: bool,
    pub protected: bool,
}

/// Entradas de la web de la página, sin secretos. Primero las del mismo host; luego las más usadas.
pub fn logins_for(s: &Session, page: &str) -> Vec<LoginView> {
    let mut hits: Vec<(&Entry, bool)> = s.vault.entries.iter().filter(|e| live(e)).filter_map(|e| site_match(e, page).map(|exact| (e, exact))).collect();
    hits.sort_by_key(|(e, exact)| (!exact, std::cmp::Reverse(e.used.unwrap_or(0))));
    hits.into_iter()
        .map(|(e, _)| LoginView {
            id: e.id.clone(),
            title: e.title.clone(),
            username: text(s, e, "username"),
            totp: e.field("totp").is_some_and(|f| !f.value.is_empty()),
            protected: e.protected,
        })
        .collect()
}

/// La entrada, solo si es de la web de la página (aunque la extensión pida otra cosa, aquí se vuelve a mirar).
fn entry_for<'a>(s: &'a Session, id: &str, page: &str) -> Res<&'a Entry> {
    let e = s.entry(id)?;
    if !live(e) || site_match(e, page).is_none() {
        return Err(AppError::new("bridge.wrongSite"));
    }
    Ok(e)
}

fn of_kind<'a>(s: &'a Session, id: &str, kind: Kind) -> Res<&'a Entry> {
    let e = s.entry(id)?;
    if !live(e) || e.kind != kind {
        return Err(AppError::new("entry.notFound"));
    }
    Ok(e)
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Check {
    New,
    Same,
    Changed { entry_id: String },
}

/// ¿Lo que se acaba de enviar en un formulario ya está guardado? (se compara aquí; a la extensión no vuelve nada)
pub fn check(s: &Session, page: &str, user: &str, pw: &str) -> Check {
    let user = user.trim();
    let hits: Vec<&Entry> = s.vault.entries.iter().filter(|e| live(e) && site_match(e, page).is_some()).collect();
    let pw_of = |e: &Entry| text(s, e, "password");
    if user.is_empty() {
        // Sin usuario a la vista (cambio de contraseña): si la web solo tiene una entrada, es esa
        return match (hits.iter().any(|e| pw_of(e) == pw), hits.as_slice()) {
            (true, _) => Check::Same,
            (false, [only]) => Check::Changed { entry_id: only.id.clone() },
            _ => Check::New,
        };
    }
    match hits.iter().find(|e| text(s, e, "username").trim().eq_ignore_ascii_case(user)) {
        Some(e) if pw_of(e) == pw => Check::Same,
        Some(e) => Check::Changed { entry_id: e.id.clone() },
        None => Check::New,
    }
}

/// `https://accounts.google.com:8443/x?y` → `https://accounts.google.com:8443`
fn origin(page: &str) -> String {
    let (scheme, rest) = page.split_once("://").unwrap_or(("https", page));
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    format!("{scheme}://{}", host.rsplit('@').next().unwrap_or(""))
}

fn field(key: &str, value: &str, secret: bool, kind: FieldKind) -> Field {
    Field { id: new_id(), key: key.into(), label: String::new(), value: Sv::text(value), secret, kind }
}

pub fn save_new(s: &mut Session, page: &str, user: &str, pw: &str) -> Res<String> {
    let t = now();
    let mut fields = vec![
        field("username", user.trim(), false, FieldKind::Text),
        field("password", pw, true, FieldKind::Password),
        field("url", &origin(page), false, FieldKind::Url),
        field("totp", "", true, FieldKind::Totp),
        field("notes", "", true, FieldKind::Multiline),
    ];
    for f in fields.iter_mut().filter(|f| f.secret) {
        f.value.seal(&s.sealer);
    }
    let id = new_id();
    s.vault.entries.push(Entry {
        id: id.clone(),
        kind: Kind::Login,
        title: registrable(page).or_else(|| host_of(page)).unwrap_or_else(|| "Web".into()),
        folder: None,
        tags: vec![],
        favorite: false,
        fields,
        attachments: vec![],
        icon: None,
        autotype: None,
        history: vec![],
        created: t,
        modified: t,
        used: Some(t),
        deleted: None,
        breach: None,
        archived: false,
        protected: false,
    });
    Ok(id)
}

pub fn update_password(s: &mut Session, id: &str, page: &str, pw: &str) -> Res<()> {
    entry_for(s, id, page)?;
    let mut value = Sv::text(pw);
    value.seal(&s.sealer);
    let e = s.entry_mut(id)?;
    let snap = commands::snapshot(e);
    e.history.insert(0, snap);
    e.history.truncate(MAX_HISTORY);
    match e.fields.iter_mut().find(|f| f.key == "password") {
        Some(f) => f.value = value,
        None => e.fields.push(Field { value, ..field("password", "", true, FieldKind::Password) }),
    }
    e.breach = None;
    e.modified = now();
    Ok(())
}

fn is_local(page: &str) -> bool {
    host_of(page).is_some_and(|h| h == "localhost" || h == "127.0.0.1" || h == "[::1]")
}

/// http sin cifrar (salvo en el propio PC)
fn insecure(page: &str) -> bool {
    page.starts_with("http://") && !is_local(page)
}

// ————————————————————————————————————————————————————————————— servidor

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum Msg {
    Hello { pair_id: Option<String>, key: Option<String> },
    Pair { browser: String, code: String },
    Unlock,
    Match { url: String, top: String },
    Fill { entry_id: String, url: String },
    Totp { entry_id: String, url: String },
    Cards { url: String },
    Identities,
    FillCard { entry_id: String, url: String },
    FillIdentity { entry_id: String },
    Check { url: String, username: String, password: String },
    Save { url: String, username: String, password: String },
    Update { entry_id: String, url: String, password: String },
    Generate,
    Copy { text: String },
}

#[derive(Default)]
struct Conn {
    pair: Mutex<Option<String>>,
}

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let name = pipe_name();
        let create = |first: bool| ServerOptions::new().first_pipe_instance(first).reject_remote_clients(true).create(&name);
        // Si el nombre está cogido (otra copia aún cerrándose) se reintenta
        let mut server = loop {
            match create(true) {
                Ok(s) => break s,
                Err(_) => tokio::time::sleep(Duration::from_secs(2)).await,
            }
        };
        loop {
            let ok = server.connect().await.is_ok();
            // La siguiente instancia se crea antes de soltar esta: el nombre nunca queda libre
            let next = loop {
                match create(false) {
                    Ok(s) => break s,
                    Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
                }
            };
            let connected = std::mem::replace(&mut server, next);
            if ok {
                tauri::async_runtime::spawn(serve(app.clone(), connected));
            }
        }
    });
}

fn ready(app: &AppHandle) -> bool {
    app.state::<AppState>().sess().as_ref().is_some_and(|s| !s.must_change_password)
}

async fn serve(app: AppHandle, pipe: NamedPipeServer) {
    if !client_pid(&pipe).is_some_and(is_our_exe) {
        return;
    }
    let (mut r, mut w) = tokio::io::split(pipe);
    let (out, mut out_rx) = mpsc::unbounded_channel::<Value>();
    tauri::async_runtime::spawn(async move {
        while let Some(v) = out_rx.recv().await {
            let b = serde_json::to_vec(&v).unwrap_or_default();
            if w.write_all(&(b.len() as u32).to_le_bytes()).await.is_err() || w.write_all(&b).await.is_err() || w.flush().await.is_err() {
                break;
            }
        }
    });
    let (in_tx, mut in_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    tauri::async_runtime::spawn(async move {
        loop {
            let mut len = [0u8; 4];
            if r.read_exact(&mut len).await.is_err() {
                break;
            }
            let n = u32::from_le_bytes(len) as usize;
            let mut buf = vec![0; n];
            if n > MAX_FRAME || r.read_exact(&mut buf).await.is_err() || in_tx.send(buf).is_err() {
                break;
            }
        }
    });
    let conn = Arc::new(Conn::default());
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut was_ready: Option<bool> = None;
    loop {
        tokio::select! {
            m = in_rx.recv() => {
                let Some(buf) = m else { break };
                let (app, conn, out) = (app.clone(), conn.clone(), out.clone());
                // Cada petición en su tarea: una espera de Hello o del emparejado no frena las demás
                tauri::async_runtime::spawn(async move {
                    let v: Value = serde_json::from_slice(&buf).unwrap_or(Value::Null);
                    let id = v.get("id").cloned().unwrap_or(Value::Null);
                    let res = match serde_json::from_value::<Msg>(v) {
                        Ok(msg) => handle(&app, &conn, msg).await,
                        Err(_) => Err(AppError::new("bridge.badRequest")),
                    };
                    let _ = out.send(match res {
                        Ok(ok) => json!({ "id": id, "ok": ok }),
                        Err(err) => json!({ "id": id, "err": err }),
                    });
                });
            }
            _ = tick.tick() => {
                // Bloquear o desbloquear la app se avisa a la extensión al momento
                if conn.pair.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                    let now_ready = ready(&app);
                    if was_ready.is_some_and(|w| w != now_ready) {
                        let _ = out.send(json!({ "event": if now_ready { "unlocked" } else { "locked" } }));
                    }
                    was_ready = Some(now_ready);
                }
            }
        }
    }
}

fn check_url(url: &str) -> Res<()> {
    check_len(url, 4096)?;
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::new("bridge.badRequest"));
    }
    Ok(())
}

fn check_id(id: &str) -> Res<()> {
    check_len(id, 64)
}

/// Entrada sensible: Windows Hello antes de soltar nada. Sin Hello, se confirma en la app (`entry.protected`).
async fn confirm(app: &AppHandle, id: &str, protected: bool) -> Res<()> {
    let st = app.state::<AppState>();
    if !protected || commands::is_verified(&st, id) {
        return Ok(());
    }
    let msg = if config::is_spanish(&st.cfg()) { "Confirma que eres tú para rellenar esta entrada" } else { "Confirm it's you to fill this entry" };
    match tauri::async_runtime::spawn_blocking(move || hello::consent(msg)).await {
        Ok(Ok(())) => {
            commands::grant(&st, id);
            Ok(())
        }
        Ok(Err(e)) if e.code == "hello.unavailable" => Err(AppError::new("entry.protected")),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(AppError::new("hello.failed")),
    }
}

async fn handle(app: &AppHandle, conn: &Conn, msg: Msg) -> Res<Value> {
    let st = app.state::<AppState>();
    if !st.cfg().bridge_enabled {
        return Err(AppError::new("bridge.disabled"));
    }
    let paired = conn.pair.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match msg {
        Msg::Hello { pair_id, key } => {
            let mut paired = paired;
            if let (Some(id), Some(key)) = (pair_id, key) {
                check_id(&id)?;
                check_len(&key, 100)?;
                if verify_pair(&st.data_dir, &id, &key) {
                    paired = Some(id);
                }
            }
            *conn.pair.lock().unwrap_or_else(|e| e.into_inner()) = paired.clone();
            let c = st.cfg().clone();
            let state = if paired.is_none() { "unpaired" } else if ready(app) { "ready" } else { "locked" };
            Ok(json!({
                "state": state,
                "version": app.package_info().version.to_string(),
                "lang": if config::is_spanish(&c) { "es" } else { "en" },
                "theme": c.theme,
                "accent": c.accent,
            }))
        }
        Msg::Pair { browser, code } => {
            check_len(&browser, 40)?;
            if code.len() != 4 || !code.chars().all(|c| c.is_ascii_digit()) {
                return Err(AppError::new("bridge.badRequest"));
            }
            let rx = {
                let mut p = PENDING_PAIR.lock().unwrap_or_else(|e| e.into_inner());
                if p.is_some() {
                    return Err(AppError::new("bridge.busy"));
                }
                let (tx, rx) = oneshot::channel();
                *p = Some(tx);
                rx
            };
            crate::show_window(app);
            let _ = app.emit("bridge-pair", json!({ "browser": browser, "code": code }));
            let answer = tokio::time::timeout(PAIR_TIMEOUT, rx).await;
            PENDING_PAIR.lock().unwrap_or_else(|e| e.into_inner()).take();
            let _ = app.emit("bridge-pair-end", ());
            match answer {
                Ok(Ok(true)) => {
                    let (id, key) = new_pair(&st.data_dir, &browser);
                    *conn.pair.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.clone());
                    Ok(json!({ "pairId": id, "key": key }))
                }
                Ok(_) => Err(AppError::new("bridge.denied")),
                Err(_) => Err(AppError::new("bridge.timeout")),
            }
        }
        msg => {
            // Lo demás, solo con un navegador emparejado (y que siga estándolo: se puede revocar en Ajustes)
            let ok = paired.as_ref().is_some_and(|id| pairs(&st.data_dir).as_ref().is_some_and(|l| l.iter().any(|p| &p.id == id)));
            if !ok {
                *conn.pair.lock().unwrap_or_else(|e| e.into_inner()) = None;
                return Err(AppError::new("bridge.notPaired"));
            }
            paired_request(app, &st, msg).await
        }
    }
}

async fn paired_request(app: &AppHandle, st: &AppState, msg: Msg) -> Res<Value> {
    match msg {
        Msg::Hello { .. } | Msg::Pair { .. } => Err(AppError::new("bridge.badRequest")),
        Msg::Unlock => {
            crate::show_window(app);
            Ok(Value::Null)
        }
        Msg::Match { url, top } => {
            check_url(&url)?;
            check_url(&top)?;
            let logins = with(st, |s| Ok(logins_for(s, &url)))?;
            let same_site = registrable(&url).is_some() && registrable(&url) == registrable(&top);
            Ok(json!({ "logins": logins, "sameSite": same_site, "insecure": insecure(&url) }))
        }
        Msg::Fill { entry_id, url } => {
            check_id(&entry_id)?;
            check_url(&url)?;
            let protected = with(st, |s| Ok(entry_for(s, &entry_id, &url)?.protected))?;
            confirm(app, &entry_id, protected).await?;
            let out = with(st, |s| {
                let e = entry_for(s, &entry_id, &url)?;
                let secret = text(s, e, "totp");
                let code = if secret.is_empty() { None } else { totp::code(&secret).ok().map(|c| c.code) };
                Ok(json!({ "username": text(s, e, "username"), "password": text(s, e, "password"), "totp": code }))
            })?;
            commands::mark_used(st, &entry_id);
            Ok(out)
        }
        Msg::Totp { entry_id, url } => {
            check_id(&entry_id)?;
            check_url(&url)?;
            let protected = with(st, |s| Ok(entry_for(s, &entry_id, &url)?.protected))?;
            confirm(app, &entry_id, protected).await?;
            with(st, |s| Ok(json!({ "code": totp::code(&text(s, entry_for(s, &entry_id, &url)?, "totp"))?.code })))
        }
        Msg::Cards { url } => {
            check_url(&url)?;
            if insecure(&url) {
                return Ok(json!([]));
            }
            with(st, |s| {
                Ok(json!(s
                    .vault
                    .entries
                    .iter()
                    .filter(|e| live(e) && e.kind == Kind::Card)
                    .map(|e| {
                        let n: String = text(s, e, "number").chars().filter(char::is_ascii_digit).collect();
                        let last4 = if n.len() >= 4 { n[n.len() - 4..].to_string() } else { String::new() };
                        json!({ "id": e.id, "title": e.title, "holder": text(s, e, "holder"), "last4": last4 })
                    })
                    .collect::<Vec<_>>()))
            })
        }
        Msg::Identities => with(st, |s| {
            Ok(json!(s
                .vault
                .entries
                .iter()
                .filter(|e| live(e) && e.kind == Kind::Identity)
                .map(|e| json!({ "id": e.id, "title": e.title, "fullname": text(s, e, "fullname"), "email": text(s, e, "email") }))
                .collect::<Vec<_>>()))
        }),
        Msg::FillCard { entry_id, url } => {
            check_id(&entry_id)?;
            check_url(&url)?;
            if insecure(&url) {
                return Err(AppError::new("bridge.insecure"));
            }
            let protected = with(st, |s| Ok(of_kind(s, &entry_id, Kind::Card)?.protected))?;
            confirm(app, &entry_id, protected).await?;
            let out = with(st, |s| {
                let e = of_kind(s, &entry_id, Kind::Card)?;
                Ok(json!({ "holder": text(s, e, "holder"), "number": text(s, e, "number"), "expiry": text(s, e, "expiry"), "cvv": text(s, e, "cvv") }))
            })?;
            commands::mark_used(st, &entry_id);
            Ok(out)
        }
        Msg::FillIdentity { entry_id } => {
            check_id(&entry_id)?;
            let protected = with(st, |s| Ok(of_kind(s, &entry_id, Kind::Identity)?.protected))?;
            confirm(app, &entry_id, protected).await?;
            let out = with(st, |s| {
                let e = of_kind(s, &entry_id, Kind::Identity)?;
                Ok(json!({ "fullname": text(s, e, "fullname"), "email": text(s, e, "email"), "phone": text(s, e, "phone"), "address": text(s, e, "address") }))
            })?;
            commands::mark_used(st, &entry_id);
            Ok(out)
        }
        Msg::Check { url, username, password } => {
            check_url(&url)?;
            check_len(&username, 500)?;
            check_len(&password, 4096)?;
            with(st, |s| Ok(json!(check(s, &url, &username, &password))))
        }
        Msg::Save { url, username, password } => {
            check_url(&url)?;
            check_len(&username, 500)?;
            check_len(&password, 4096)?;
            if password.is_empty() {
                return Err(AppError::new("bridge.badRequest"));
            }
            let id = mutate(st, |s| save_new(s, &url, &username, &password))?;
            let _ = app.emit("bridge-saved", ());
            Ok(json!({ "id": id }))
        }
        Msg::Update { entry_id, url, password } => {
            check_id(&entry_id)?;
            check_url(&url)?;
            check_len(&password, 4096)?;
            if password.is_empty() {
                return Err(AppError::new("bridge.badRequest"));
            }
            let protected = with(st, |s| Ok(entry_for(s, &entry_id, &url)?.protected))?;
            confirm(app, &entry_id, protected).await?;
            mutate(st, |s| update_password(s, &entry_id, &url, &password))?;
            let _ = app.emit("bridge-saved", ());
            Ok(Value::Null)
        }
        Msg::Generate => {
            let opts = st.cfg().generator.clone();
            let pw = zeroize::Zeroizing::new(generator::generate(&opts).value);
            commands::remember_generated(st, &pw);
            Ok(json!({ "password": pw.as_str() }))
        }
        Msg::Copy { text } => {
            let text = zeroize::Zeroizing::new(text);
            check_len(&text, 4096)?;
            let secs = st.cfg().clipboard_secs;
            clipboard::copy_secret(&text, (secs > 0).then(|| Duration::from_secs(secs as u64)))?;
            Ok(json!(secs))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::file::{self, tests::{tmpdir, FAST}};

    fn login(title: &str, user: &str, url: &str, pw: &str) -> Entry {
        Entry {
            id: new_id(),
            kind: Kind::Login,
            title: title.into(),
            folder: None,
            tags: vec![],
            favorite: false,
            fields: vec![field("username", user, false, FieldKind::Text), field("password", pw, true, FieldKind::Password), field("url", url, false, FieldKind::Url)],
            attachments: vec![],
            icon: None,
            autotype: None,
            history: vec![],
            created: 1,
            modified: 1,
            used: None,
            deleted: None,
            breach: None,
            archived: false,
            protected: false,
        }
    }

    fn session(entries: Vec<Entry>) -> Session {
        let d = tmpdir("bridge");
        let path = d.join("b.poxipass");
        let (o, _) = file::create(&path, Vault { entries, ..Default::default() }, "pw", FAST, &d.join("bk")).unwrap();
        Session::new(path, o)
    }

    #[test]
    fn matches_by_registrable_domain_exact_host_first() {
        let mut archived = login("Viejo", "yo", "https://github.com", "0");
        archived.archived = true;
        let s = session(vec![
            login("GitHub gist", "gist", "https://gist.github.com", "1"),
            login("GitHub", "yo", "https://github.com/login", "2"),
            login("Falso", "yo", "https://github.com.evil.io", "3"),
            login("Pages", "yo", "https://alguien.github.io", "4"),
            archived,
        ]);
        let titles: Vec<String> = logins_for(&s, "https://github.com/session").into_iter().map(|l| l.title).collect();
        assert_eq!(titles, ["GitHub", "GitHub gist"], "mismo host primero; ni phishing ni archivadas");
        assert!(logins_for(&s, "https://otro.github.io").is_empty(), "cada página de github.io es de alguien distinto");
        assert_eq!(logins_for(&s, "https://github.com")[0].username, "yo");
    }

    #[test]
    fn secrets_only_for_the_page_domain() {
        let s = session(vec![login("Banco", "yo", "https://mibanco.es", "secreto")]);
        let id = s.vault.entries[0].id.clone();
        assert!(entry_for(&s, &id, "https://www.mibanco.es/login").is_ok());
        assert_eq!(entry_for(&s, &id, "https://mibanco.es.phish.com/").err().unwrap().code, "bridge.wrongSite");
    }

    #[test]
    fn check_save_and_update() {
        let mut s = session(vec![login("GitHub", "Yo", "https://github.com", "vieja")]);
        let id = s.vault.entries[0].id.clone();
        let page = "https://github.com/login?x=1";
        assert_eq!(check(&s, page, "yo ", "vieja"), Check::Same);
        assert_eq!(check(&s, page, "yo", "nueva"), Check::Changed { entry_id: id.clone() });
        assert_eq!(check(&s, page, "otra", "x"), Check::New);
        assert_eq!(check(&s, page, "", "vieja"), Check::Same);
        assert_eq!(check(&s, page, "", "otra"), Check::Changed { entry_id: id.clone() }, "cambio de contraseña sin usuario");
        assert_eq!(check(&s, "https://gitlab.com", "yo", "vieja"), Check::New);

        update_password(&mut s, &id, page, "nueva").unwrap();
        assert_eq!(check(&s, page, "yo", "nueva"), Check::Same);
        assert_eq!(s.vault.entries[0].history.len(), 1, "la anterior queda en el historial");
        assert!(update_password(&mut s, &id, "https://evil.com", "x").is_err());

        let new = save_new(&mut s, "https://accounts.example.co.uk:8443/signup?a=b", "ana", "pw1").unwrap();
        let e = s.entry(&new).unwrap();
        assert_eq!(e.title, "example.co.uk");
        assert_eq!(e.field("url").unwrap().value.plain_str(), "https://accounts.example.co.uk:8443");
        assert!(e.field("password").unwrap().value.is_sealed());
        assert_eq!(check(&s, "https://example.co.uk", "ana", "pw1"), Check::Same);
    }

    #[test]
    fn pairing_keys_are_hashed_and_compared() {
        let d = tmpdir("pairs");
        *PAIRS.lock().unwrap() = None;
        let (id, key) = new_pair(&d, "Edge");
        assert!(verify_pair(&d, &id, &key));
        assert!(!verify_pair(&d, &id, "otra"));
        assert!(!verify_pair(&d, "otro", &key));
        let disk = std::fs::read_to_string(d.join("browsers.json")).unwrap();
        assert!(!disk.contains(&key) && disk.contains(&hash_key(&key)), "en disco solo el hash");
        *PAIRS.lock().unwrap() = None;
    }

    #[test]
    fn insecure_pages() {
        assert!(insecure("http://tienda.com/pago"));
        assert!(!insecure("https://tienda.com"));
        assert!(!insecure("http://localhost:5173/"));
        assert_eq!(origin("https://user@a.b.com/x"), "https://a.b.com");
    }

    #[test]
    fn tiendas() {
        // Sin IDs publicados, en desarrollo se abre la búsqueda de la tienda que toca
        assert_eq!(store_url("edge").as_deref(), Some("https://microsoftedge.microsoft.com/addons/search/PoxiPass"));
        assert_eq!(store_url("brave").as_deref(), Some("https://chromewebstore.google.com/search/PoxiPass"));
    }

    #[test]
    #[ignore = "depende de los navegadores instalados"]
    fn navegadores_instalados() {
        let found: Vec<_> = extension_targets().iter().map(|b| b.name).collect();
        println!("{found:?}");
        assert!(!found.is_empty());
    }
}
