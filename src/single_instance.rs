//! Single-instance guard: a second launch focuses the existing window and exits.

pub const WINDOW_TITLE: &str = "الذكاء المحلي";

/// Holds the named mutex for the lifetime of the app.
pub struct Guard {
    #[cfg(windows)]
    _handle: windows_sys::Win32::Foundation::HANDLE,
}

/// Returns `Some(guard)` if this is the first instance, `None` if another one
/// is already running (after bringing it to the foreground).
#[cfg(windows)]
pub fn acquire() -> Option<Guard> {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };

    let name = wide("Local\\LocalAiChat.SingleInstance");
    // SAFETY: plain Win32 calls with valid, NUL-terminated UTF-16 strings.
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return Some(Guard { _handle: handle });
        }
        let title = wide(WINDOW_TITLE);
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd);
        }
        None
    }
}

#[cfg(not(windows))]
pub fn acquire() -> Option<Guard> {
    Some(Guard {})
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
