//! Portapapeles para secretos: no entra en el historial de Win+V ni se sube a la nube de Windows, y se borra solo
//! (si no has copiado otra cosa entretanto).

use crate::error::{AppError, Res};
use std::time::Duration;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use zeroize::Zeroize;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn open() -> bool {
    // Otra app puede tenerlo abierto un instante
    for _ in 0..20 {
        if OpenClipboard(std::ptr::null_mut()) != 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    false
}

unsafe fn put(format: u32, bytes: &[u8]) -> bool {
    let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
    if h.is_null() {
        return false;
    }
    let p = GlobalLock(h) as *mut u8;
    if p.is_null() {
        GlobalFree(h);
        return false;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    GlobalUnlock(h);
    if SetClipboardData(format, h).is_null() {
        GlobalFree(h);
        return false;
    }
    true
}

pub fn copy_secret(text: &str, clear_after: Option<Duration>) -> Res<()> {
    let mut utf16 = wide(text);
    let seq = unsafe {
        if !open() {
            utf16.zeroize();
            return Err(AppError::new("clipboard.busy"));
        }
        EmptyClipboard();
        let bytes = std::slice::from_raw_parts(utf16.as_ptr() as *const u8, utf16.len() * 2);
        let ok = put(CF_UNICODETEXT as u32, bytes);
        let zero = 0u32.to_le_bytes();
        put(RegisterClipboardFormatW(wide("ExcludeClipboardContentFromMonitorProcessing").as_ptr()), &zero);
        put(RegisterClipboardFormatW(wide("CanIncludeInClipboardHistory").as_ptr()), &zero);
        put(RegisterClipboardFormatW(wide("CanUploadToCloudClipboard").as_ptr()), &zero);
        CloseClipboard();
        utf16.zeroize();
        if !ok {
            return Err(AppError::new("clipboard.busy"));
        }
        GetClipboardSequenceNumber()
    };
    if let Some(after) = clear_after {
        std::thread::spawn(move || {
            std::thread::sleep(after);
            clear_if_unchanged(seq);
        });
    }
    Ok(())
}

/// Borra el portapapeles solo si sigue teniendo lo que pusimos nosotros.
pub fn clear_if_unchanged(seq: u32) {
    unsafe {
        if GetClipboardSequenceNumber() == seq && open() {
            EmptyClipboard();
            CloseClipboard();
        }
    }
}

/// Texto del portapapeles (para el «Pegar» del menú contextual propio de la app).
pub fn read_text() -> Res<String> {
    unsafe {
        if !open() {
            return Err(AppError::new("clipboard.busy"));
        }
        let h = GetClipboardData(CF_UNICODETEXT as u32);
        let mut out = String::new();
        if !h.is_null() {
            let p = GlobalLock(h) as *const u16;
            if !p.is_null() {
                let mut len = 0;
                while *p.add(len) != 0 && len < 1_000_000 {
                    len += 1;
                }
                out = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
                GlobalUnlock(h);
            }
        }
        CloseClipboard();
        Ok(out)
    }
}
