//! Native window (tao) hosting the UI in a WebView2 webview (wry).

use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

use crate::config::Paths;
use crate::single_instance::WINDOW_TITLE;

pub fn run(url: &str, paths: &Paths) -> anyhow::Result<()> {
    // User event `()` = quit (sent by the updater from the server thread).
    let event_loop = EventLoop::new();
    let proxy = std::sync::Mutex::new(event_loop.create_proxy());
    crate::update::set_exit_handler(move || {
        let _ = proxy.lock().map(|p| p.send_event(()));
    });
    let builder = WindowBuilder::new();
    // Big (taskbar / Alt+Tab) icon too: tao only sets the small one by default.
    #[cfg(windows)]
    let builder = {
        use tao::platform::windows::WindowBuilderExtWindows;
        builder.with_taskbar_icon(app_icon_sized(256))
    };
    let window = builder
        .with_title(WINDOW_TITLE)
        .with_inner_size(LogicalSize::new(1200.0, 820.0))
        .with_min_inner_size(LogicalSize::new(420.0, 500.0))
        .with_window_icon(app_icon())
        .build(&event_loop)?;

    // Keep the WebView2 profile out of the exe folder.
    let mut context = WebContext::new(Some(paths.webview_dir.clone()));
    let app_url = url.to_string();

    let _webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(url)
        .with_background_color((31, 30, 29, 255))
        // External links open in the default browser instead of inside the app.
        .with_navigation_handler(move |target| {
            if is_app_url(&app_url, &target) {
                return true;
            }
            open_external(&target);
            false
        })
        .with_new_window_req_handler(|target, _| {
            open_external(&target);
            wry::NewWindowResponse::Deny
        })
        .build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            }
            | Event::UserEvent(()) => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

/// The icon embedded by `build.rs` (winresource uses resource id 1).
#[cfg(windows)]
fn app_icon() -> Option<tao::window::Icon> {
    use tao::platform::windows::IconExtWindows;
    tao::window::Icon::from_resource(1, None).ok()
}

/// The embedded icon at a specific size (Windows picks or scales the best entry).
#[cfg(windows)]
fn app_icon_sized(px: u32) -> Option<tao::window::Icon> {
    use tao::platform::windows::IconExtWindows;
    tao::window::Icon::from_resource(1, Some(tao::dpi::PhysicalSize::new(px, px))).ok()
}

#[cfg(not(windows))]
fn app_icon() -> Option<tao::window::Icon> {
    None
}

/// `target` has exactly the app's origin (scheme + host + port). A prefix
/// check would accept `http://127.0.0.1:5123@evil.com/` or
/// `http://127.0.0.1:51234/` for an app on port 5123.
fn is_app_url(app_url: &str, target: &str) -> bool {
    match (reqwest::Url::parse(app_url), reqwest::Url::parse(target)) {
        (Ok(app), Ok(t)) => {
            app.origin().is_tuple()
                && app.origin() == t.origin()
                && t.username().is_empty()
                && t.password().is_none()
        }
        _ => false,
    }
}

fn open_external(url: &str) {
    if url.starts_with("http://") || url.starts_with("https://") {
        let _ = std::process::Command::new("explorer").arg(url).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::is_app_url;

    #[test]
    fn app_url_requires_exact_origin() {
        let app = "http://127.0.0.1:5123/";
        assert!(is_app_url(app, "http://127.0.0.1:5123/"));
        assert!(is_app_url(app, "http://127.0.0.1:5123"));
        assert!(is_app_url(app, "http://127.0.0.1:5123/index.html?x=1#y"));
        assert!(!is_app_url(app, "http://127.0.0.1:5123@evil.com/"));
        assert!(!is_app_url(app, "http://127.0.0.1:51234/"));
        assert!(!is_app_url(app, "http://127.0.0.1:512/"));
        assert!(!is_app_url(app, "https://127.0.0.1:5123/"));
        assert!(!is_app_url(app, "http://localhost:5123/"));
        assert!(!is_app_url(app, "http://127.0.0.1.evil.com:5123/"));
        assert!(!is_app_url(app, "http://user:pw@127.0.0.1:5123/"));
        assert!(!is_app_url(app, "https://example.com/"));
        assert!(!is_app_url(app, "about:blank"));
        assert!(!is_app_url(app, "not a url"));
    }
}
