//! Crash log: records panics to `<data dir>\crash.txt`.
//!
//! The release exe has no console, so the default panic message goes nowhere.
//! [`install`] adds a panic hook that appends a short entry (UTC time, app
//! version, thread, message, location, backtrace) to `crash.txt` in the data
//! folder (`%APPDATA%\LocalAiChat`, or `LOCALAI_DATA_DIR`), then runs the
//! previous (default) hook, so behaviour is otherwise unchanged. The file keeps
//! only the newest ~64 KB of whole entries. The release build is stripped, so
//! its backtraces show addresses rather than function names.
//!
//! The hook never panics: every IO error is ignored. On the next start the UI
//! asks [`unseen`] (via `/api/app/crash`) and shows a one-line notice.

use std::cell::Cell;
use std::fmt::Write as _;
use std::panic::{self, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// `crash.txt` keeps at most about this many bytes (whole entries, newest last).
const MAX_BYTES: usize = 64 * 1024;
const CRASH_FILE: &str = "crash.txt";
/// Touched when the UI has shown the notice; `crash.txt` newer than it = unseen.
const SEEN_FILE: &str = "crash.seen";
/// First line of every entry; also how the size cap finds entry boundaries.
const ENTRY_START: &str = "=== Lemon crash ";

static PATH: OnceLock<PathBuf> = OnceLock::new();
/// Two threads panicking at once must not interleave their read-modify-write.
static WRITE: Mutex<()> = Mutex::new(());

thread_local! {
    /// Already inside the hook on this thread.
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}

/// `<data dir>\crash.txt`.
pub fn crash_file(data_dir: &Path) -> PathBuf {
    data_dir.join(CRASH_FILE)
}

/// Installs the hook for the app's data folder. Only the first call counts.
pub fn install() {
    if let Ok(paths) = crate::config::Paths::resolve() {
        install_at(crash_file(&paths.data_dir));
    }
}

fn install_at(path: PathBuf) {
    if PATH.set(path).is_err() {
        return;
    }
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if !IN_HOOK.try_with(|h| h.replace(true)).unwrap_or(true) {
            if let Some(path) = PATH.get() {
                record(path, info);
            }
            let _ = IN_HOOK.try_with(|h| h.set(false));
        }
        previous(info);
    }));
}

fn record(path: &Path, info: &PanicHookInfo<'_>) {
    let thread = std::thread::current();
    let name = thread.name().map_or_else(|| format!("{:?}", thread.id()), str::to_string);
    let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
    let backtrace = std::backtrace::Backtrace::force_capture().to_string();
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let message = info.payload_as_str().unwrap_or("(no message)");
    let text = entry(secs, &name, message, location.as_deref(), &backtrace);
    append(path, &text, MAX_BYTES);
}

/// One entry. Every line but the first is indented, so only real entry starts
/// begin with [`ENTRY_START`].
fn entry(secs: u64, thread: &str, message: &str, location: Option<&str>, backtrace: &str) -> String {
    let mut s = format!("{ENTRY_START}{} v{} ===\n", utc(secs), env!("CARGO_PKG_VERSION"));
    let _ = writeln!(s, "  thread: {thread}");
    let mut lines = message.lines();
    let _ = writeln!(s, "  message: {}", lines.next().unwrap_or(""));
    for line in lines {
        let _ = writeln!(s, "    {line}");
    }
    let _ = writeln!(s, "  location: {}", location.unwrap_or("unknown"));
    s.push_str("  backtrace:\n");
    for line in backtrace.lines() {
        let _ = writeln!(s, "    {line}");
    }
    s.push('\n');
    s
}

/// `YYYY-MM-DD HH:MM:SS UTC` for Unix seconds (civil-from-days, H. Hinnant).
fn utc(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (h, m, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02} UTC")
}

/// `old` + `entry`, dropping the oldest whole entries until it fits in `max`
/// bytes. An entry bigger than `max` on its own is cut and marked.
fn capped(old: &str, entry: &str, max: usize) -> String {
    let all = format!("{old}{entry}");
    if all.len() <= max {
        return all;
    }
    let min_start = all.len() - max;
    let start = old
        .match_indices(ENTRY_START)
        .map(|(i, _)| i)
        .filter(|&i| i >= min_start && (i == 0 || old.as_bytes()[i - 1] == b'\n'))
        .min()
        .unwrap_or(old.len());
    let mut kept = all[start..].to_string();
    if kept.len() > max {
        let mut end = max;
        while !kept.is_char_boundary(end) {
            end -= 1;
        }
        kept.truncate(end);
        kept.push_str("\n  [truncated]\n\n");
    }
    kept
}

/// Adds `entry` to the file at `path`, keeping it under `max`. Ignores errors.
fn append(path: &Path, entry: &str, max: usize) {
    let _lock = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let old = std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let _ = crate::config::write_atomic(path, capped(&old, entry, max).as_bytes());
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// `crash.txt` changed since the UI last showed the notice ([`mark_seen`]).
pub fn unseen(data_dir: &Path) -> bool {
    match (modified(&crash_file(data_dir)), modified(&data_dir.join(SEEN_FILE))) {
        (Some(crash), Some(seen)) => crash > seen,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Records that the notice for the current `crash.txt` was shown.
pub fn mark_seen(data_dir: &Path) -> std::io::Result<()> {
    std::fs::write(data_dir.join(SEEN_FILE), utc(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lac-crash-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn utc_formats_known_dates() {
        assert_eq!(utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(utc(1_793_712_896), "2026-11-03 13:34:56 UTC");
    }

    #[test]
    fn entry_has_every_field() {
        let e = entry(0, "worker", "boom\n=== Lemon crash fake", Some("src/x.rs:3:9"), "   0: main\n   1: start");
        assert!(e.starts_with(&format!("=== Lemon crash 1970-01-01 00:00:00 UTC v{} ===\n", env!("CARGO_PKG_VERSION"))));
        assert!(e.contains("\n  thread: worker\n  message: boom\n    === Lemon crash fake\n"), "{e}");
        assert!(e.contains("\n  location: src/x.rs:3:9\n  backtrace:\n       0: main\n       1: start\n"), "{e}");
        assert_eq!(e.matches("\n=== Lemon crash").count(), 0);
        assert!(entry(0, "t", "", None, "").contains("location: unknown"));
    }

    #[test]
    fn size_cap_keeps_newest_whole_entries() {
        let one = entry(1, "a", &"x".repeat(300), None, "");
        let mut file = String::new();
        for _ in 0..50 {
            file = capped(&file, &one, 2000);
            assert!(file.len() <= 2000);
            assert!(file.starts_with(ENTRY_START));
        }
        assert_eq!(file.matches(ENTRY_START).count(), 2000 / one.len());
        let newest = entry(2, "b", "last", None, "");
        assert!(capped(&file, &newest, 2000).ends_with(&newest));
        // Foreign text without entries is dropped; an oversized entry is cut on a char boundary.
        assert_eq!(capped(&"?".repeat(3000), &newest, 2000), newest);
        let huge = entry(3, "c", &"ب".repeat(5000), None, "");
        let cut = capped("", &huge, 2001);
        assert!(cut.starts_with(ENTRY_START) && cut.ends_with("[truncated]\n\n") && cut.len() < 2100);
    }

    #[test]
    fn append_ignores_unwritable_paths_and_seen_marker_works() {
        let dir = temp_dir("io");
        std::fs::create_dir_all(dir.join("crash.txt")).unwrap(); // a folder, not a file
        append(&dir.join("crash.txt"), "x", MAX_BYTES);
        append(Path::new(""), "x", MAX_BYTES);
        let data = dir.join("data");
        assert!(!unseen(&data));
        append(&crash_file(&data), "entry\n", MAX_BYTES);
        assert!(unseen(&data));
        mark_seen(&data).unwrap();
        assert!(!unseen(&data));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Integration-style: a child test process with a temp `LOCALAI_DATA_DIR`
    /// installs the real hook and panics in a named thread; the parent checks
    /// `crash.txt`. (A child process, so no env var is changed in this one.)
    #[test]
    fn hook_writes_crash_txt_in_data_dir() {
        const CHILD: &str = "LAC_CRASH_LOG_CHILD";
        if std::env::var_os(CHILD).is_some() {
            install();
            let t = std::thread::Builder::new().name("probe-thread".into());
            let r = t.spawn(|| panic!("probe panic {}", 6 * 7)).unwrap().join();
            assert!(r.is_err());
            return;
        }
        let dir = temp_dir("hook");
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash_log::tests::hook_writes_crash_txt_in_data_dir", "--nocapture"])
            .env(CHILD, "1")
            .env(crate::config::DATA_DIR_ENV, &dir)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        // The default hook still ran after ours.
        assert!(stderr.contains("probe panic 42"), "{stderr}");
        let text = std::fs::read_to_string(dir.join("crash.txt")).unwrap();
        assert!(text.starts_with(ENTRY_START), "{text}");
        assert!(text.contains(&format!(" v{} ===", env!("CARGO_PKG_VERSION"))), "{text}");
        assert!(text.contains("  thread: probe-thread\n  message: probe panic 42\n"), "{text}");
        assert!(text.contains("  location: src") && text.contains("crash_log.rs:"), "{text}");
        assert!(text.contains("  backtrace:\n    "), "{text}");
        assert!(unseen(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
