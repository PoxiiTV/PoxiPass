//! Códigos 2FA (TOTP). El campo guarda o la URL `otpauth://…` completa o la clave en base32.

use crate::error::{AppError, Res};
use serde::Serialize;
use totp_rs::{Algorithm, Secret, TOTP};

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Code {
    pub code: String,
    pub remaining: u64,
    pub period: u64,
}

pub fn parse(input: &str) -> Res<TOTP> {
    let s = input.trim();
    let bad = || AppError::new("totp.invalid");
    if s.to_ascii_lowercase().starts_with("otpauth://") {
        return TOTP::from_url_unchecked(s).map_err(|_| bad());
    }
    let clean: String = s.chars().filter(|c| !c.is_whitespace() && *c != '-' && *c != '=').collect::<String>().to_uppercase();
    let bytes = Secret::Encoded(clean).to_bytes().map_err(|_| bad())?;
    if bytes.len() < 5 {
        return Err(bad());
    }
    Ok(TOTP::new_unchecked(Algorithm::SHA1, 6, 1, 30, bytes, None, String::new()))
}

pub fn code_at(input: &str, unix: u64) -> Res<Code> {
    let t = parse(input)?;
    Ok(Code { code: t.generate(unix), remaining: t.step - unix % t.step, period: t.step })
}

pub fn code(input: &str) -> Res<Code> {
    code_at(input, crate::util::now() as u64)
}

/// Busca un QR `otpauth://` en una imagen (captura pegada o archivo).
pub fn from_image(bytes: &[u8]) -> Res<String> {
    let img = image::load_from_memory(bytes).map_err(|_| AppError::new("totp.badImage"))?.to_luma8();
    let (w, h) = img.dimensions();
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w as usize, h as usize, |x, y| img.get_pixel(x as u32, y as u32).0[0]);
    prepared
        .detect_grids()
        .into_iter()
        .filter_map(|g| g.decode().ok().map(|(_, s)| s))
        .find(|s| s.to_ascii_lowercase().starts_with("otpauth://") && parse(s).is_ok())
        .ok_or_else(|| AppError::new("totp.noQr"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_vector() {
        let t = TOTP::new_unchecked(Algorithm::SHA1, 8, 1, 30, b"12345678901234567890".to_vec(), None, String::new());
        assert_eq!(t.generate(59), "94287082");
        assert_eq!(t.generate(1_111_111_109), "07081804");
    }

    #[test]
    fn base32_and_uri() {
        // «12345678901234567890» en base32, escrito como lo pegaría una persona
        let b32 = "gezd gnbv gy3t qojq gezd gnbv gy3t qojq";
        let c = code_at(b32, 59).unwrap();
        assert_eq!((c.code.as_str(), c.remaining, c.period), ("287082", 1, 30));
        let uri = "otpauth://totp/Ejemplo:poxi?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Ejemplo&digits=8&period=30";
        assert_eq!(code_at(uri, 59).unwrap().code, "94287082");
        assert!(parse("esto no es base32 !!").is_err());
        assert!(parse("").is_err());
    }
}
