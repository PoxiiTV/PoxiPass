//! Windows Hello: una credencial del TPM (`PoxiPass-<vault_id>`) firma un desafío fijo. La firma RSA (PKCS#1 v1.5)
//! siempre sale igual, así que su SHA-256 sirve de clave para envolver la Vault Key. La cerradura se guarda en
//! la configuración de este PC, nunca en el archivo de la bóveda.

use crate::config::HelloSlot;
use crate::crypto::{self, random, Key};
use crate::error::{AppError, Res};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use sha2::{Digest, Sha256};
use windows::core::{Array, HSTRING};
use windows::Security::Credentials::{KeyCredential, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus};
use windows::Security::Cryptography::CryptographicBuffer;
use zeroize::Zeroizing;

/// Días que se puede abrir con Hello sin volver a poner la contraseña maestra
pub const MAX_DAYS: i64 = 14;

fn name(vault_id: &str) -> HSTRING {
    HSTRING::from(format!("PoxiPass-{vault_id}"))
}

fn aad(vault_id: &str) -> Vec<u8> {
    format!("poxipass-hello:{vault_id}").into_bytes()
}

fn status_err(s: KeyCredentialStatus) -> AppError {
    AppError::new(match s {
        KeyCredentialStatus::UserCanceled => "hello.cancelled",
        KeyCredentialStatus::NotFound => "hello.notFound",
        _ => "hello.failed",
    })
}

fn w<T>(r: windows::core::Result<T>) -> Res<T> {
    r.map_err(|_| AppError::new("hello.failed"))
}

pub fn available() -> bool {
    KeyCredentialManager::IsSupportedAsync().and_then(|op| op.get()).unwrap_or(false)
}

/// El diálogo de Hello a veces sale detrás de la ventana: se le trae al frente en cuanto aparece.
fn bring_dialog_to_front() {
    std::thread::spawn(|| {
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow};
        let class: Vec<u16> = "Credential Dialog Xaml Host".encode_utf16().chain([0]).collect();
        for _ in 0..40 {
            let h = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
            if !h.is_null() {
                unsafe { SetForegroundWindow(h) };
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}

fn sign(cred: &KeyCredential, challenge: &[u8]) -> Res<Key> {
    let buf = w(CryptographicBuffer::CreateFromByteArray(challenge))?;
    bring_dialog_to_front();
    let res = w(w(cred.RequestSignAsync(&buf))?.get())?;
    let status = w(res.Status())?;
    if status != KeyCredentialStatus::Success {
        return Err(status_err(status));
    }
    let mut sig = Array::<u8>::new();
    w(CryptographicBuffer::CopyToByteArray(&w(res.Result())?, &mut sig))?;
    let mut k = Zeroizing::new([0u8; 32]);
    k.copy_from_slice(&Sha256::digest(&sig[..]));
    Ok(k)
}

/// Crea la credencial (pide huella/cara/PIN) y envuelve la Vault Key.
pub fn enroll(vault_id: &str, key: &Key) -> Res<HelloSlot> {
    bring_dialog_to_front();
    let res = w(w(KeyCredentialManager::RequestCreateAsync(&name(vault_id), KeyCredentialCreationOption::ReplaceExisting))?.get())?;
    let status = w(res.Status())?;
    if status != KeyCredentialStatus::Success {
        return Err(status_err(status));
    }
    let cred = w(res.Credential())?;
    let challenge = random::<32>();
    let kek = sign(&cred, &challenge)?;
    Ok(HelloSlot {
        challenge: B64.encode(challenge),
        wrapped: B64.encode(crypto::seal(&kek, key.as_ref(), &aad(vault_id))),
        last_password: crate::util::now(),
    })
}

pub fn unlock(vault_id: &str, slot: &HelloSlot) -> Res<Key> {
    if crate::util::now() - slot.last_password > MAX_DAYS * 86_400 {
        return Err(AppError::new("hello.expired"));
    }
    let res = w(w(KeyCredentialManager::OpenAsync(&name(vault_id)))?.get())?;
    let status = w(res.Status())?;
    if status != KeyCredentialStatus::Success {
        return Err(status_err(status));
    }
    let challenge = B64.decode(&slot.challenge).map_err(|_| AppError::new("hello.failed"))?;
    let kek = sign(&w(res.Credential())?, &challenge)?;
    let wrapped = B64.decode(&slot.wrapped).map_err(|_| AppError::new("hello.failed"))?;
    let raw = crypto::open(&kek, &wrapped, &aad(vault_id)).ok_or_else(|| AppError::new("hello.failed"))?;
    let mut k = Zeroizing::new([0u8; 32]);
    k.copy_from_slice(&raw[..32]);
    Ok(k)
}

pub fn remove(vault_id: &str) {
    if let Ok(op) = KeyCredentialManager::DeleteAsync(&name(vault_id)) {
        let _ = op.get();
    }
}

/// Pide huella/cara/PIN solo para confirmar que eres tú (entradas sensibles). `hello.unavailable` si este PC no
/// tiene Windows Hello (entonces la interfaz pide la contraseña maestra).
pub fn consent(message: &str) -> Res<()> {
    use windows::Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability};
    let avail = w(w(UserConsentVerifier::CheckAvailabilityAsync())?.get())?;
    if avail != UserConsentVerifierAvailability::Available {
        return Err(AppError::new("hello.unavailable"));
    }
    bring_dialog_to_front();
    match w(w(UserConsentVerifier::RequestVerificationAsync(&HSTRING::from(message)))?.get())? {
        UserConsentVerificationResult::Verified => Ok(()),
        UserConsentVerificationResult::Canceled => Err(AppError::new("hello.cancelled")),
        _ => Err(AppError::new("hello.failed")),
    }
}
