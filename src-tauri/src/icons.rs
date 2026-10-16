//! Favicons (solo si se activan en Ajustes): se bajan directamente de la web, se validan como imagen y se guardan
//! como PNG de 64×64 dentro de la bóveda cifrada.

use crate::error::{AppError, Res};
use std::io::Cursor;
use std::time::Duration;

const MAX_BYTES: usize = 256 * 1024;

pub fn host(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let h = rest.split(['/', '?', '#']).next()?.to_lowercase();
    (h.contains('.') && h.chars().all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c))).then_some(h)
}

/// Decodifica cualquier formato soportado (ico, png, jpg…) y lo deja en PNG 64×64.
pub fn to_png64(bytes: &[u8]) -> Res<Vec<u8>> {
    let img = image::load_from_memory(bytes).map_err(|_| AppError::new("icons.bad"))?;
    let small = img.resize(64, 64, image::imageops::FilterType::Lanczos3);
    let mut out = Cursor::new(vec![]);
    small.write_to(&mut out, image::ImageFormat::Png).map_err(|_| AppError::new("icons.bad"))?;
    Ok(out.into_inner())
}

pub async fn fetch(client: &reqwest::Client, url: &str) -> Res<Vec<u8>> {
    let h = host(url).ok_or_else(|| AppError::new("icons.bad"))?;
    for path in ["favicon.ico", "apple-touch-icon.png"] {
        let Ok(resp) = client.get(format!("https://{h}/{path}")).timeout(Duration::from_secs(5)).send().await else { continue };
        if !resp.status().is_success() || resp.content_length().is_some_and(|l| l as usize > MAX_BYTES) {
            continue;
        }
        let Ok(bytes) = resp.bytes().await else { continue };
        if bytes.len() <= MAX_BYTES {
            if let Ok(png) = to_png64(&bytes) {
                return Ok(png);
            }
        }
    }
    Err(AppError::new("icons.notFound"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_and_resize() {
        assert_eq!(host("https://www.netflix.com/es/login").as_deref(), Some("www.netflix.com"));
        assert_eq!(host("github.com").as_deref(), Some("github.com"));
        assert_eq!(host("javascript:alert(1)"), None);
        assert_eq!(host("nada"), None);
        let mut src = Cursor::new(vec![]);
        image::DynamicImage::new_rgba8(200, 100).write_to(&mut src, image::ImageFormat::Png).unwrap();
        let png = to_png64(src.get_ref()).unwrap();
        let back = image::load_from_memory(&png).unwrap();
        assert_eq!((back.width(), back.height()), (64, 32));
        assert!(to_png64(b"<html>").is_err());
    }
}
