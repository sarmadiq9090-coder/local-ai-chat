//! Local AI Chat — a Windows desktop chat client for Ollama.
//!
//! Architecture:
//! - `server`  : local HTTP server (axum) serving the embedded UI and the JSON API
//! - `ollama`  : Ollama client (health check, auto-start)
//! - `storage` : conversations persisted as JSON files
//! - `config`  : user settings + data paths
//! - `system`  : hardware summary (GPU/VRAM, RAM, CPU) for model suggestions
//! - `window`  : native window hosting the UI in WebView2
//!
//! The server runs on a tokio runtime in a background thread; the window event
//! loop owns the main thread (required by Windows).

// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assistants;
mod config;
mod error;
mod ollama;
mod server;
mod single_instance;
mod storage;
mod system;
mod update;
mod window;

use std::sync::mpsc;

fn main() -> anyhow::Result<()> {
    // Developer mode: `local-ai-chat --serve [port]` runs only the server (no
    // window, no single-instance lock). In debug builds the UI is read from
    // `ui/` on disk, so UI edits show up on browser refresh without rebuilding.
    let args: Vec<String> = std::env::args().collect();
    // Started by the updater: clean up the old exe, tell the UI (see update.rs).
    let after_update = args.iter().any(|a| a == update::AFTER_UPDATE_ARG);
    if after_update {
        update::after_update();
    }
    if args.get(1).is_some_and(|a| a == "--serve") {
        let port = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(8790);
        update::set_relaunch_args(vec!["--serve".into(), port.to_string()]);
        if after_update {
            update::wait_for_port(port);
        }
        return serve_only(port);
    }

    // After an update the old instance may still be exiting: wait for its mutex.
    let instance = if after_update {
        single_instance::acquire_wait(std::time::Duration::from_secs(10))
    } else {
        single_instance::acquire()
    };
    let Some(_instance) = instance else {
        return Ok(()); // already running: the existing window was focused
    };
    let paths = config::Paths::resolve()?;
    let (port_tx, port_rx) = mpsc::channel();

    let server_paths = paths.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            if let Err(e) = server::run(server_paths, 0, port_tx).await {
                eprintln!("server error: {e:#}");
            }
        });
    });

    let port = port_rx.recv()?;
    window::run(&format!("http://127.0.0.1:{port}/"), &paths)
}

fn serve_only(port: u16) -> anyhow::Result<()> {
    let paths = config::Paths::resolve()?;
    let (port_tx, port_rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok(port) = port_rx.recv() {
            println!("Local AI Chat (dev) listening on http://127.0.0.1:{port}/");
        }
    });
    tokio::runtime::Runtime::new()?.block_on(server::run(paths, port, port_tx))
}
