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

/// Like [`acquire`], but first waits up to `timeout` for a running instance to
/// exit (used after an update, while the old process is shutting down).
#[cfg(windows)]
pub fn acquire_wait(timeout: std::time::Duration) -> Option<Guard> {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name = wide("Local\\LocalAiChat.SingleInstance");
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        // SAFETY: as in `acquire`; the handle to an existing mutex is closed so
        // the object can disappear once the old process exits.
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if GetLastError() != ERROR_ALREADY_EXISTS {
                return Some(Guard { _handle: handle });
            }
            if !handle.is_null() {
                CloseHandle(handle);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    acquire()
}

#[cfg(not(windows))]
pub fn acquire_wait(_timeout: std::time::Duration) -> Option<Guard> {
    acquire()
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
