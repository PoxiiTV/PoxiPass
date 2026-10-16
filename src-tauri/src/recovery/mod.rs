//! Kit de emergencia: 256 bits de azar escritos como 24 palabras de la Lista Poxi (igual que BIP39: 8 bits de
//! checksum, 11 bits por palabra). Con un 50 % salen además los comodines «maricón» y «z», que no cuentan.

use crate::crypto::{random, uniform};
use crate::error::AppError;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::LazyLock;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

pub const WORD_COUNT: usize = 24;
pub const JOKERS: [&str; 2] = ["maricón", "z"];

pub static WORDS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| include_str!("wordlist.txt").lines().filter(|l| !l.is_empty()).collect());

/// Prefijo (4 letras sin tildes, o la palabra entera si tiene 3) → posición en la lista
static INDEX: LazyLock<HashMap<String, u16>> =
    LazyLock::new(|| WORDS.iter().enumerate().map(|(i, w)| (key(&normalize(w)), i as u16)).collect());

/// Minúsculas y sin tildes: «Cabezón» → «cabezon», «ñoño» → «nono».
pub fn normalize(w: &str) -> String {
    w.nfd().filter(|c| !is_combining_mark(*c)).flat_map(char::to_lowercase).collect()
}

fn key(normalized: &str) -> String {
    normalized.chars().take(4).collect()
}

pub struct Phrase {
    /// Tal cual se enseñan e imprimen (con tildes y, a veces, con comodines)
    pub words: Vec<String>,
    pub entropy: Zeroizing<[u8; 32]>,
}

pub fn generate() -> Phrase {
    let entropy = Zeroizing::new(random::<32>());
    let mut words = encode(&entropy);
    if uniform(2) == 0 {
        for joker in JOKERS {
            let at = uniform(words.len() as u32 + 1) as usize;
            words.insert(at, joker.to_string());
        }
    }
    Phrase { words, entropy }
}

fn checked_bits(entropy: &[u8; 32]) -> Zeroizing<[u8; 33]> {
    let mut data = Zeroizing::new([0u8; 33]);
    data[..32].copy_from_slice(entropy);
    data[32] = Sha256::digest(entropy)[0];
    data
}

fn bit(data: &[u8], i: usize) -> u16 {
    ((data[i / 8] >> (7 - i % 8)) & 1) as u16
}

pub fn encode(entropy: &[u8; 32]) -> Vec<String> {
    let data = checked_bits(entropy);
    (0..WORD_COUNT)
        .map(|w| {
            let idx = (0..11).fold(0u16, |v, b| (v << 1) | bit(&data[..], w * 11 + b));
            WORDS[idx as usize].to_string()
        })
        .collect()
}

/// Lee lo que escribe la persona: da igual mayúsculas, tildes, números, comas o comodines, y basta con las
/// 4 primeras letras de cada palabra.
pub fn parse(input: &str) -> Result<Zeroizing<[u8; 32]>, AppError> {
    let normalized = Zeroizing::new(normalize(input));
    let tokens: Vec<&str> = normalized
        .split(|c: char| !c.is_alphabetic())
        .filter(|t| !t.is_empty() && *t != "maricon" && *t != "z")
        .collect();
    if tokens.len() != WORD_COUNT {
        return Err(AppError::with("recovery.wrongCount", tokens.len().to_string()));
    }
    let mut data = Zeroizing::new([0u8; 33]);
    for (w, t) in tokens.iter().enumerate() {
        let idx = *INDEX
            .get(&key(t))
            .ok_or_else(|| AppError::with("recovery.unknownWord", (w + 1).to_string()))?;
        for b in 0..11 {
            if (idx >> (10 - b)) & 1 == 1 {
                let i = w * 11 + b;
                data[i / 8] |= 1 << (7 - i % 8);
            }
        }
    }
    let mut entropy = Zeroizing::new([0u8; 32]);
    entropy.copy_from_slice(&data[..32]);
    if Sha256::digest(entropy.as_ref())[0] != data[32] {
        return Err(AppError::new("recovery.badChecksum"));
    }
    Ok(entropy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn wordlist_invariants() {
        assert_eq!(WORDS.len(), 2048);
        let mut keys = HashSet::new();
        let mut norms = HashSet::new();
        for w in WORDS.iter() {
            let n = normalize(w);
            assert!((3..=10).contains(&n.chars().count()), "{w}: longitud");
            assert_eq!(*w, w.to_lowercase(), "{w}: mayúsculas");
            assert!(n.chars().all(|c| c.is_ascii_lowercase()), "{w}: caracteres raros");
            assert!(n != "maricon" && n != "z", "{w}: es un comodín");
            assert!(keys.insert(key(&n)), "{w}: prefijo repetido");
            assert!(norms.insert(n), "{w}: repetida sin tildes");
        }
    }

    #[test]
    fn roundtrip() {
        for _ in 0..50 {
            let p = generate();
            assert!(p.words.len() == 24 || p.words.len() == 26);
            assert_eq!(*parse(&p.words.join(" ")).unwrap(), *p.entropy);
        }
    }

    #[test]
    fn jokers_appear_about_half_the_time() {
        let with = (0..400).filter(|_| generate().words.len() == 26).count();
        assert!((120..=280).contains(&with), "{with}");
    }

    #[test]
    fn forgiving_input() {
        let entropy = [42u8; 32];
        let words = encode(&entropy);
        let messy: Vec<String> = words
            .iter()
            .enumerate()
            .map(|(i, w)| match i % 3 {
                0 => w.to_uppercase(),
                1 => normalize(w).chars().take(4).collect(),
                _ => format!("{}. {w},", i + 1),
            })
            .collect();
        let input = format!("  maricón {}  Z ", messy.join("   "));
        assert_eq!(*parse(&input).unwrap(), entropy);
    }

    #[test]
    fn errors_say_what_is_wrong() {
        let mut words = encode(&[1u8; 32]);
        assert_eq!(parse(&words[..23].join(" ")).unwrap_err().code, "recovery.wrongCount");
        words[4] = "qqqqq".into();
        let e = parse(&words.join(" ")).unwrap_err();
        assert_eq!((e.code, e.detail.as_deref()), ("recovery.unknownWord", Some("5")));
        let mut words = encode(&[1u8; 32]);
        words.swap(0, 1);
        if words[0] != words[1] {
            assert_eq!(parse(&words.join(" ")).unwrap_err().code, "recovery.badChecksum");
        }
    }
}
