//! Lo que se le pregunta a Windows: inactividad, sesión bloqueada y ventana activa.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::RemoteDesktop::{
    WTSFreeMemory, WTSQuerySessionInformationW, WTSSessionInfoEx, WTSINFOEXW, WTS_CURRENT_SERVER_HANDLE,
    WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, IsWindow, SetForegroundWindow,
};

/// Milisegundos desde la última pulsación o movimiento de ratón en todo el PC.
pub fn idle_ms() -> u32 {
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        if GetLastInputInfo(&mut info) == 0 {
            return 0;
        }
        GetTickCount().wrapping_sub(info.dwTime)
    }
}

/// ¿Está la sesión de Windows bloqueada (Win+L)?
pub fn session_locked() -> bool {
    unsafe {
        let mut buf: *mut u16 = std::ptr::null_mut();
        let mut len = 0u32;
        if WTSQuerySessionInformationW(WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTSSessionInfoEx, &mut buf, &mut len) == 0
            || buf.is_null()
        {
            return false;
        }
        let info = &*(buf as *const WTSINFOEXW);
        let locked = info.Level == 1 && info.Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_LOCK as i32;
        WTSFreeMemory(buf.cast());
        locked
    }
}

pub fn foreground() -> isize {
    unsafe { GetForegroundWindow() as isize }
}

pub fn window_title(hwnd: isize) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd as HWND, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn focus(hwnd: isize) -> bool {
    unsafe { IsWindow(hwnd as HWND) != 0 && SetForegroundWindow(hwnd as HWND) != 0 }
}

/// Rectángulo (izquierda, arriba, derecha, abajo) en píxeles reales
pub type Rect = (i32, i32, i32, i32);

/// Posición del ratón y zona útil (sin la barra de tareas) de la pantalla en la que está, en píxeles reales.
pub fn cursor_and_work_area() -> Option<((i32, i32), Rect)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    unsafe {
        let mut p = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut p) == 0 {
            return None;
        }
        let mon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut info) == 0 {
            return None;
        }
        let w = info.rcWork;
        Some(((p.x, p.y), (w.left, w.top, w.right, w.bottom)))
    }
}

/// Dónde poner una ventana de `size` para que salga justo debajo del ratón, centrada en él, y siempre dentro
/// de la zona útil de su pantalla (si no cabe debajo, sube).
pub fn place_below(cursor: (i32, i32), size: (i32, i32), area: Rect) -> (i32, i32) {
    const GAP: i32 = 12;
    let (l, t, r, b) = area;
    let x = (cursor.0 - size.0 / 2).clamp(l, (r - size.0).max(l));
    let below = cursor.1 + GAP;
    let y = if below + size.1 <= b { below } else { (cursor.1 - GAP - size.1).max(t) };
    (x, y.min((b - size.1).max(t)))
}

#[cfg(test)]
mod tests {
    use super::place_below;

    #[test]
    fn quick_bar_stays_on_screen() {
        let area = (0, 0, 1920, 1040);
        let size = (680, 460);
        // En medio: centrada en el ratón y 12 px por debajo
        assert_eq!(place_below((960, 300), size, area), (620, 312));
        // Pegado a la izquierda / derecha: no se sale
        assert_eq!(place_below((5, 300), size, area).0, 0);
        assert_eq!(place_below((1915, 300), size, area).0, 1240);
        // Abajo del todo: sale por encima del ratón
        assert_eq!(place_below((960, 1000), size, area), (620, 528));
        // Segunda pantalla a la izquierda (coordenadas negativas)
        assert_eq!(place_below((-960, 200), size, (-1920, 0, 0, 1040)), (-1300, 212));
    }
}
