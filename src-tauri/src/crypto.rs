//! Primitivas: Argon2id, XChaCha20-Poly1305, HKDF, HMAC y el `Sealer` que guarda los secretos sellados en RAM.

use crate::error::{AppError, Res};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use unicode_normalization::UnicodeNormalization;
use zeroize::{Zeroize, Zeroizing};

pub type Key = Zeroizing<[u8; 32]>;

const NONCE: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct KdfParams {
    /// KiB de memoria
    pub m: u32,
    pub t: u32,
    pub p: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self { m: 64 * 1024, t: 3, p: 4 }
    }
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("el generador aleatorio del sistema no responde");
    b
}

pub fn random_key() -> Key {
    Zeroizing::new(random::<32>())
}

/// Número uniforme en `[0, n)` (descarta los valores que sesgarían el módulo).
pub fn uniform(n: u32) -> u32 {
    assert!(n > 0);
    let n = n as u64;
    let limit = (1u64 << 32) - ((1u64 << 32) % n);
    loop {
        let v = u32::from_le_bytes(random::<4>()) as u64;
        if v < limit {
            return (v % n) as u32;
        }
    }
}

/// Fisher-Yates con `uniform`.
pub fn shuffle<T>(v: &mut [T]) {
    for i in (1..v.len()).rev() {
        v.swap(i, uniform(i as u32 + 1) as usize);
    }
}

/// Clave a partir de la contraseña maestra. Se normaliza a NFC: «ñ» escrita como un carácter o como «n + ~» abre igual.
pub fn derive_password_key(password: &str, salt: &[u8], kdf: &KdfParams) -> Res<Key> {
    let pw = Zeroizing::new(password.nfc().collect::<String>());
    let params = Params::new(kdf.m, kdf.t, kdf.p, Some(32)).map_err(|_| AppError::new("vault.corrupt"))?;
    let mut out = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pw.as_bytes(), salt, out.as_mut())
        .map_err(|_| AppError::new("vault.corrupt"))?;
    Ok(out)
}

pub fn hkdf_key(ikm: &[u8], salt: &[u8], info: &[u8]) -> Key {
    let mut out = Zeroizing::new([0u8; 32]);
    hkdf::Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, out.as_mut())
        .expect("32 bytes siempre caben en HKDF-SHA256");
    out
}

pub fn hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC acepta cualquier longitud de clave");
    m.update(data);
    m.finalize().into_bytes().into()
}

/// Cifra con nonce aleatorio. Devuelve `nonce || texto cifrado`.
pub fn seal(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    let nonce = random::<NONCE>();
    let ct = XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad })
        .expect("XChaCha20-Poly1305 no falla al cifrar");
    let mut out = Vec::with_capacity(NONCE + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

/// Descifra `nonce || texto cifrado`. Clave incorrecta o datos tocados: `None` (no se distingue a propósito).
pub fn open(key: &[u8; 32], data: &[u8], aad: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    if data.len() < NONCE + 16 {
        return None;
    }
    let (nonce, ct) = data.split_at(NONCE);
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad })
        .ok()
        .map(Zeroizing::new)
}

/// Sella los secretos de la bóveda abierta con una clave de sesión aleatoria. Esa clave vive cifrada con
/// `CryptProtectMemory` (solo este proceso la puede abrir) y se descifra en una copia temporal en cada uso.
pub struct Sealer {
    blob: Box<[u8; 32]>,
}

impl Sealer {
    pub fn new() -> Self {
        let mut blob = Box::new(random::<32>());
        protect(blob.as_mut(), true);
        Self { blob }
    }

    fn key(&self) -> Key {
        let mut k = Zeroizing::new(*self.blob);
        protect(&mut k, false);
        k
    }

    pub fn seal(&self, data: &[u8]) -> Vec<u8> {
        seal(&self.key(), data, b"poxipass-ram")
    }

    pub fn open(&self, blob: &[u8]) -> Zeroizing<Vec<u8>> {
        open(&self.key(), blob, b"poxipass-ram").expect("un secreto sellado en esta sesión siempre se abre")
    }

    pub fn open_string(&self, blob: &[u8]) -> Zeroizing<String> {
        let bytes = self.open(blob);
        Zeroizing::new(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Huella de un secreto para compararlo con otros sin abrirlos a la vez (contraseñas repetidas).
    pub fn fingerprint(&self, data: &[u8]) -> [u8; 32] {
        hmac(self.key().as_ref(), data)
    }
}

impl Default for Sealer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Sealer {
    fn drop(&mut self) {
        self.blob.zeroize();
    }
}

#[cfg(windows)]
fn protect(buf: &mut [u8; 32], on: bool) {
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectMemory, CryptUnprotectMemory, CRYPTPROTECTMEMORY_SAME_PROCESS,
    };
    // 32 es múltiplo de CRYPTPROTECTMEMORY_BLOCK_SIZE (16)
    let ok = unsafe {
        if on {
            CryptProtectMemory(buf.as_mut_ptr().cast(), 32, CRYPTPROTECTMEMORY_SAME_PROCESS)
        } else {
            CryptUnprotectMemory(buf.as_mut_ptr().cast(), 32, CRYPTPROTECTMEMORY_SAME_PROCESS)
        }
    };
    assert!(ok != 0, "CryptProtectMemory ha fallado");
}

#[cfg(not(windows))]
fn protect(_: &mut [u8; 32], _: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: KdfParams = KdfParams { m: 1024, t: 1, p: 1 };

    #[test]
    fn seal_roundtrip_and_aad() {
        let k = random_key();
        let ct = seal(&k, b"hola", b"a");
        assert_eq!(open(&k, &ct, b"a").unwrap().as_slice(), b"hola");
        assert!(open(&k, &ct, b"b").is_none());
        assert!(open(&random_key(), &ct, b"a").is_none());
        let mut bad = ct.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(open(&k, &bad, b"a").is_none());
        assert!(open(&k, &ct[..10], b"a").is_none());
    }

    #[test]
    fn password_key_is_unicode_normalized() {
        let salt = [7u8; 16];
        let nfc = derive_password_key("contrase\u{00f1}a 🔐", &salt, &FAST).unwrap();
        let nfd = derive_password_key("contrasen\u{0303}a 🔐", &salt, &FAST).unwrap();
        assert_eq!(*nfc, *nfd);
        assert_ne!(*nfc, *derive_password_key("contrasena 🔐", &salt, &FAST).unwrap());
    }

    #[test]
    fn sealer_roundtrip() {
        let s = Sealer::new();
        let b = s.seal(b"secreto");
        assert_eq!(s.open(&b).as_slice(), b"secreto");
        assert_eq!(s.fingerprint(b"x"), s.fingerprint(b"x"));
        assert_ne!(s.fingerprint(b"x"), Sealer::new().fingerprint(b"x"));
    }

    #[test]
    fn uniform_stays_in_range() {
        for n in [1u32, 2, 3, 7, 62, 2048] {
            for _ in 0..200 {
                assert!(uniform(n) < n);
            }
        }
    }
}
