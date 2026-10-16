//! Generador de contraseñas y frases, y medidor de fortaleza (zxcvbn).

use crate::crypto::{shuffle, uniform};
use crate::recovery::{normalize, WORDS};
use serde::{Deserialize, Serialize};

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.<>?/~";
const AMBIGUOUS: &str = "Il1O0o";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GenOptions {
    /// `password`, `passphrase` o `username`
    pub mode: String,
    /// Caracteres que no deben salir nunca (webs que no aceptan algunos símbolos)
    pub exclude: String,
    pub length: u32,
    pub lower: bool,
    pub upper: bool,
    pub digits: bool,
    pub symbols: bool,
    pub avoid_ambiguous: bool,
    pub words: u32,
    pub separator: String,
    pub capitalize: bool,
    pub number: bool,
}

impl Default for GenOptions {
    fn default() -> Self {
        Self {
            mode: "password".into(),
            exclude: String::new(),
            length: 20,
            lower: true,
            upper: true,
            digits: true,
            symbols: true,
            avoid_ambiguous: false,
            words: 5,
            separator: "-".into(),
            capitalize: true,
            number: true,
        }
    }
}

impl GenOptions {
    pub fn clamped(mut self) -> Self {
        self.length = self.length.clamp(4, 128);
        self.words = self.words.clamp(3, 12);
        if !matches!(self.separator.as_str(), "-" | "." | "_" | " " | "") {
            self.separator = "-".into();
        }
        if !(self.lower || self.upper || self.digits || self.symbols) {
            self.lower = true;
        }
        if !matches!(self.mode.as_str(), "passphrase" | "username") {
            self.mode = "password".into();
        }
        self.exclude = self.exclude.chars().filter(|c| !c.is_whitespace()).take(60).collect();
        self
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Generated {
    pub value: String,
    pub bits: f64,
}

fn pick(chars: &[char]) -> char {
    chars[uniform(chars.len() as u32) as usize]
}

pub fn generate(opts: &GenOptions) -> Generated {
    let o = opts.clone().clamped();
    match o.mode.as_str() {
        "passphrase" => passphrase(&o),
        "username" => username(),
        _ => password(&o),
    }
}

fn password(o: &GenOptions) -> Generated {
    let sets: Vec<Vec<char>> = [(o.lower, LOWER), (o.upper, UPPER), (o.digits, DIGITS), (o.symbols, SYMBOLS)]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, s)| s.chars().filter(|c| (!o.avoid_ambiguous || !AMBIGUOUS.contains(*c)) && !o.exclude.contains(*c)).collect::<Vec<char>>())
        .filter(|s| !s.is_empty())
        .collect();
    // Si se ha excluido todo, al menos minúsculas
    let sets = if sets.is_empty() { vec![LOWER.chars().filter(|c| !o.exclude.contains(*c)).collect::<Vec<char>>()] } else { sets };
    let all: Vec<char> = sets.concat();
    // Uno de cada conjunto marcado, el resto de todos, y se baraja
    let mut out: Vec<char> = sets.iter().map(|s| pick(s)).collect();
    while out.len() < o.length as usize {
        out.push(pick(&all));
    }
    shuffle(&mut out);
    Generated { value: out.into_iter().collect(), bits: o.length as f64 * (all.len() as f64).log2() }
}

/// Nombre de usuario de la Lista Poxi: `zascandil.torrezno27` (no revela nada de ti)
fn username() -> Generated {
    let w = || normalize(WORDS[uniform(WORDS.len() as u32) as usize]);
    let seps = ['.', '_', '-'];
    let value = format!("{}{}{}{}", w(), seps[uniform(3) as usize], w(), uniform(100));
    Generated { value, bits: 22.0 + (300f64).log2() }
}

fn passphrase(o: &GenOptions) -> Generated {
    let mut words: Vec<String> = (0..o.words)
        .map(|_| {
            let w = normalize(WORDS[uniform(WORDS.len() as u32) as usize]);
            if o.capitalize {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            } else {
                w
            }
        })
        .collect();
    let mut bits = o.words as f64 * 11.0;
    if o.number {
        let at = uniform(words.len() as u32) as usize;
        words[at].push_str(&uniform(100).to_string());
        bits += (100f64).log2() + (o.words as f64).log2();
    }
    Generated { value: words.join(&o.separator), bits }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Strength {
    /// 0 (malísima) a 4 (muy fuerte)
    pub score: u8,
    /// Segundos para crackearla con hashes lentos robados (10⁴ intentos/s)
    pub crack_secs: f64,
    pub bits: f64,
}

pub fn strength(s: &str) -> Strength {
    if s.is_empty() {
        return Strength { score: 0, crack_secs: 0.0, bits: 0.0 };
    }
    // zxcvbn se vuelve lento con textos muy largos; 100 caracteres ya dan para saber si es fuerte
    let cut: String = s.chars().take(100).collect();
    let e = zxcvbn::zxcvbn(&cut, &[]);
    Strength {
        score: e.score().into(),
        crack_secs: match e.crack_times().offline_slow_hashing_1e4_per_second() {
            zxcvbn::time_estimates::CrackTimeSeconds::Integer(i) => i as f64,
            zxcvbn::time_estimates::CrackTimeSeconds::Float(f) => f,
        },
        bits: e.guesses_log10() * std::f64::consts::LOG2_10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_respects_options() {
        for len in [4u32, 5, 20, 128] {
            let o = GenOptions { length: len, avoid_ambiguous: true, ..Default::default() };
            let p = generate(&o).value;
            assert_eq!(p.chars().count(), len as usize);
            assert!(p.chars().any(|c| c.is_ascii_lowercase()));
            assert!(p.chars().any(|c| c.is_ascii_uppercase()));
            assert!(p.chars().any(|c| c.is_ascii_digit()));
            assert!(p.chars().any(|c| SYMBOLS.contains(c)));
            assert!(!p.chars().any(|c| AMBIGUOUS.contains(c)), "{p}");
        }
        let only_digits = GenOptions { lower: false, upper: false, symbols: false, length: 30, ..Default::default() };
        assert!(generate(&only_digits).value.chars().all(|c| c.is_ascii_digit()));
        let none = GenOptions { lower: false, upper: false, digits: false, symbols: false, ..Default::default() };
        assert!(generate(&none).value.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn passphrase_uses_the_list() {
        let o = GenOptions { mode: "passphrase".into(), words: 6, capitalize: false, number: false, ..Default::default() };
        let g = generate(&o);
        let parts: Vec<&str> = g.value.split('-').collect();
        assert_eq!(parts.len(), 6);
        assert!(parts.iter().all(|p| WORDS.iter().any(|w| normalize(w) == *p)), "{}", g.value);
        assert_eq!(g.bits, 66.0);
    }

    #[test]
    fn exclude_and_username() {
        let o = GenOptions { exclude: "@#$aeiou".into(), length: 64, ..Default::default() };
        let p = generate(&o).value;
        assert!(!p.chars().any(|c| "@#$aeiou".contains(c)), "{p}");
        let u = generate(&GenOptions { mode: "username".into(), ..Default::default() }).value;
        assert!(u.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c)), "{u}");
    }

    #[test]
    fn strength_scores() {
        assert!(strength("123456").score <= 1);
        assert!(strength("Zascandil-Torrezno-Petardo-Cabezon-42").score >= 3);
        assert_eq!(strength("").score, 0);
    }
}
