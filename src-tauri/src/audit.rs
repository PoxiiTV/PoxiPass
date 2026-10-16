//! Salud de la bóveda: contraseñas débiles, repetidas, antiguas, sin 2FA y filtradas (Have I Been Pwned).

use crate::crypto::hmac;
use crate::error::{AppError, Res};
use crate::generator::strength;
use crate::session::Session;
use crate::util::now;
use crate::vault::model::Kind;
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::collections::HashMap;

const OLD_DAYS: i64 = 365;

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub score: u8,
    pub total: usize,
    pub weak: Vec<String>,
    pub reused: Vec<Vec<String>>,
    pub old: Vec<String>,
    pub no2fa: Vec<String>,
    pub breached: Vec<(String, u32)>,
    pub duplicates: Vec<Vec<String>>,
    /// Tarjetas y documentos caducados o que caducan en menos de 60 días (id, días que faltan; negativo = ya)
    pub expiring: Vec<(String, i64)>,
    /// Cuándo se comprobaron las filtraciones por última vez (la más antigua), 0 si nunca
    pub breach_checked: i64,
}

/// Huella de una contraseña para saber si el resultado de HIBP sigue valiendo (cambia si cambias la contraseña).
pub fn breach_fp(key: &[u8; 32], password: &[u8]) -> [u8; 32] {
    hmac(key, password)
}

pub fn report(s: &Session) -> Report {
    let mut r = Report::default();
    let mut groups: HashMap<[u8; 32], Vec<String>> = HashMap::new();
    let mut bad = 0f64;
    let mut checked = i64::MAX;
    let mut dupes: HashMap<(String, String), Vec<String>> = HashMap::new();
    for e in s.vault.entries.iter().filter(|e| e.deleted.is_none() && !e.archived) {
        let user = e.field("username").map(|f| f.value.plain_str().trim().to_lowercase()).unwrap_or_default();
        let site = e.field("url").and_then(|f| crate::autotype::registrable(f.value.plain_str())).unwrap_or_default();
        let pwfp = e.field("password").filter(|f| !f.value.is_empty()).map(|f| hex(&s.sealer.fingerprint(&f.value.reveal(&s.sealer)))).unwrap_or_default();
        // Misma web y mismo usuario, o (sin web) mismo título, usuario y contraseña
        let key = if !site.is_empty() && !user.is_empty() { (site, user) } else { (e.title.trim().to_lowercase(), format!("{user}|{pwfp}")) };
        if !key.0.is_empty() {
            dupes.entry(key).or_default().push(e.id.clone());
        }
        if let Some(day) = e.field("expiry").and_then(|f| crate::util::expiry_day(f.value.plain_str())) {
            let left = day - now() / 86_400;
            if left <= 60 {
                r.expiring.push((e.id.clone(), left));
            }
        }
        let Some(pw) = e.field("password").filter(|f| !f.value.is_empty()) else { continue };
        let pw = pw.value.reveal(&s.sealer);
        r.total += 1;
        let mut penalty = 0f64;
        if strength(&String::from_utf8_lossy(&pw)).score < 3 {
            r.weak.push(e.id.clone());
            penalty = 1.0;
        }
        groups.entry(s.sealer.fingerprint(&pw)).or_default().push(e.id.clone());
        if now() - e.modified > OLD_DAYS * 86_400 {
            r.old.push(e.id.clone());
            penalty = f64::max(penalty, 0.5);
        }
        let has_totp = e.field("totp").is_some_and(|f| !f.value.is_empty());
        if e.kind == Kind::Login && !has_totp && e.field("url").is_some_and(|f| !f.value.is_empty()) {
            r.no2fa.push(e.id.clone());
        }
        match &e.breach {
            Some(b) if b.fp.as_slice() == breach_fp(&s.key, &pw) => {
                checked = checked.min(b.checked);
                if b.count > 0 {
                    r.breached.push((e.id.clone(), b.count));
                    penalty = 1.0;
                }
            }
            _ => checked = 0,
        }
        bad += penalty;
    }
    r.duplicates = dupes.into_values().filter(|g| g.len() > 1).collect();
    for ids in groups.into_values().filter(|g| g.len() > 1) {
        bad += ids.len() as f64 * 0.75;
        r.reused.push(ids);
    }
    r.breach_checked = if r.total == 0 || checked == i64::MAX { 0 } else { checked };
    r.score = if r.total == 0 { 100 } else { (100.0 * (1.0 - (bad / r.total as f64).min(1.0))).round() as u8 };
    r
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// SHA-1 en hexadecimal mayúsculas (lo que entiende la API de rangos de HIBP).
pub fn sha1_hex(data: &[u8]) -> String {
    Sha1::digest(data).iter().map(|b| format!("{b:02X}")).collect()
}

/// Busca el sufijo en la respuesta de `range/<prefijo>`. Las líneas con `:0` son relleno (`Add-Padding`).
pub fn count_in_range(body: &str, suffix: &str) -> u32 {
    body.lines()
        .filter_map(|l| l.trim().split_once(':'))
        .find(|(s, _)| s.eq_ignore_ascii_case(suffix))
        .and_then(|(_, n)| n.trim().parse().ok())
        .unwrap_or(0)
}

pub async fn fetch_range(client: &reqwest::Client, prefix: &str) -> Res<String> {
    client
        .get(format!("https://api.pwnedpasswords.com/range/{prefix}"))
        .header("Add-Padding", "true")
        .header("User-Agent", "PoxiPass")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|_| AppError::new("net.offline"))?
        .text()
        .await
        .map_err(|_| AppError::new("net.offline"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_parsing_ignores_padding() {
        let h = sha1_hex(b"password");
        assert_eq!(h, "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8");
        let body = "0018A45C4D1DEF81644B54AB7F969B88D65:1\r\n1E4C9B93F3F0682250B6CF8331B7EE68FD8:9659365\r\nFFFF0000000000000000000000000000000:0";
        assert_eq!(count_in_range(body, &h[5..]), 9_659_365);
        assert_eq!(count_in_range(body, "FFFF0000000000000000000000000000000"), 0);
        assert_eq!(count_in_range(body, "ABC"), 0);
    }
}
