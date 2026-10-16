//! URL de la pestaña activa de un navegador, leída de su barra de direcciones con UI Automation (sin extensión).
//! Así el auto-type casa por dominio exacto en vez de por el título de la ventana.

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_EditControlTypeId,
    UIA_ValuePatternId,
};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

const BROWSERS: [&str; 7] = ["chrome.exe", "msedge.exe", "brave.exe", "firefox.exe", "opera.exe", "vivaldi.exe", "librewolf.exe"];

/// Nombre del ejecutable dueño de la ventana (`chrome.exe`…)
fn process_name(hwnd: isize) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd as _, &mut pid);
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().map(|s| s.to_lowercase())
    }
}

pub fn is_browser(hwnd: isize) -> bool {
    process_name(hwnd).is_some_and(|n| BROWSERS.contains(&n.as_str()))
}

/// La barra de direcciones es el primer cuadro de texto de la ventana (va antes que el contenido de la página).
pub fn url(hwnd: isize) -> Option<String> {
    if !is_browser(hwnd) {
        return None;
    }
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let root = uia.ElementFromHandle(HWND(hwnd as _)).ok()?;
        let mut v = VARIANT::default();
        (*v.Anonymous.Anonymous).vt = VT_I4;
        (*v.Anonymous.Anonymous).Anonymous.lVal = UIA_EditControlTypeId.0;
        let cond = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &v).ok()?;
        let edit = root.FindFirst(TreeScope_Descendants, &cond).ok()?;
        let value: IUIAutomationValuePattern = edit.GetCurrentPatternAs(UIA_ValuePatternId).ok()?;
        let s = value.CurrentValue().ok()?.to_string();
        let s = s.trim();
        (!s.is_empty() && !s.contains(' ') && s.contains('.')).then(|| s.to_string())
    }
}
