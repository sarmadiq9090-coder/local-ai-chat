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
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(WINDOW_TITLE)
        .with_inner_size(LogicalSize::new(1200.0, 820.0))
        .with_min_inner_size(LogicalSize::new(420.0, 500.0))
        .with_window_icon(app_icon())
        .build(&event_loop)?;

    // Keep the WebView2 profile out of the exe folder.
    let mut context = WebContext::new(Some(paths.webview_dir.clone()));
    let app_origin = url.trim_end_matches('/').to_string();

    let _webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(url)
        .with_background_color((31, 30, 29, 255))
        // External links open in the default browser instead of inside the app.
        .with_navigation_handler(move |target| {
            if target.starts_with(&app_origin) {
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
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            *control_flow = ControlFlow::Exit;
        }
    });
}

/// The icon embedded by `build.rs` (winresource uses resource id 1).
#[cfg(windows)]
fn app_icon() -> Option<tao::window::Icon> {
    use tao::platform::windows::IconExtWindows;
    tao::window::Icon::from_resource(1, None).ok()
}

#[cfg(not(windows))]
fn app_icon() -> Option<tao::window::Icon> {
    None
}

fn open_external(url: &str) {
    if url.starts_with("http://") || url.starts_with("https://") {
        let _ = std::process::Command::new("explorer").arg(url).spawn();
    }
}
