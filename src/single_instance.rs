//! Single-instance guard: a second launch focuses the existing window and exits.

pub const WINDOW_TITLE: &str = "Lemon";
/// Window title of versions before the rename (<= 2.1.1), still matched when
/// focusing an already running instance.
const LEGACY_TITLE: &str = "الذكاء المحلي";

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

    let name = wide("Local\\LocalAiChat.SingleInstance");
    // SAFETY: plain Win32 call with a valid, NUL-terminated UTF-16 string.
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return Some(Guard { _handle: handle });
        }
    }
    focus_existing_window();
    None
}

/// Finds the running instance's main window and brings it to the foreground.
///
/// `FindWindowW(title)` is not enough: other top-level windows can share the
/// title (e.g. Explorer's `Windows.Internal.Shell.TabProxyWindow`). So only
/// windows with exactly our title, owned by another process running an exe
/// with our file name, are considered.
#[cfg(windows)]
fn focus_existing_window() {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
        SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    struct Search {
        titles: Vec<Vec<u16>>,
        exe_name: String,
        own_pid: u32,
        found: HWND,
    }

    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `lparam` is the `&mut Search` handed to `EnumWindows` below,
        // alive for the whole (synchronous) enumeration.
        let search = unsafe { &mut *(lparam as *mut Search) };
        // SAFETY: `hwnd` comes from the system; buffers are sized as passed.
        unsafe {
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 || !search.titles.iter().any(|t| t.len() == len as usize) {
                return 1;
            }
            let mut buf = vec![0u16; len as usize + 1];
            let got = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if got <= 0 || !search.titles.iter().any(|t| buf[..got as usize] == t[..]) {
                return 1;
            }
        }
        let pid = pid_of(hwnd);
        if pid == 0 || pid == search.own_pid {
            return 1;
        }
        match process_exe_name(pid) {
            Some(name) if name.eq_ignore_ascii_case(&search.exe_name) => {
                search.found = hwnd;
                0 // stop enumerating
            }
            _ => 1,
        }
    }

    fn pid_of(hwnd: HWND) -> u32 {
        let mut pid = 0u32;
        // SAFETY: `hwnd` comes from the enumeration; `pid` is a valid out pointer.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        pid
    }

    let Some(exe_name) = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
    else {
        return;
    };
    let mut search = Search {
        titles: [WINDOW_TITLE, LEGACY_TITLE]
            .iter()
            .map(|t| t.encode_utf16().collect())
            .collect(),
        exe_name,
        own_pid: std::process::id(),
        found: std::ptr::null_mut(),
    };
    // SAFETY: `visit` only dereferences `lparam` as `search`, which outlives
    // this synchronous call; the found HWND is used right away.
    unsafe {
        EnumWindows(Some(visit), &mut search as *mut Search as LPARAM);
        let hwnd = search.found;
        if hwnd.is_null() {
            return;
        }
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        } else {
            ShowWindow(hwnd, SW_SHOW);
        }
        SetForegroundWindow(hwnd);
    }
}

/// File name (e.g. `local-ai-chat.exe`) of the exe running as process `pid`.
#[cfg(windows)]
fn process_exe_name(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    // SAFETY: the handle is checked and closed; the buffer length is passed in/out.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 32_768];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
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

/// Native error dialog (blocking). Used when the app can't start at all,
/// e.g. after an update while the previous instance never exited.
#[cfg(windows)]
pub fn show_error(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_OK, MB_RTLREADING, MB_SETFOREGROUND, MessageBoxW,
    };
    let text = wide(text);
    let title = wide(WINDOW_TITLE);
    // SAFETY: valid NUL-terminated UTF-16 strings; no owner window.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND | MB_RTLREADING,
        );
    }
}

#[cfg(not(windows))]
pub fn show_error(text: &str) {
    eprintln!("{text}");
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
