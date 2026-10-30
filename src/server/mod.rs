//! Local HTTP server: embedded UI + JSON API + Ollama passthrough.
//!
//! Routes:
//! - `/api/app/*`    app endpoints, one file per feature in `routes/` (merged in `routes/mod.rs`)
//! - `/api/ollama/*` forwarded to Ollama (see `ollama_proxy`)
//! - everything else: embedded static UI files (see `assets`)

mod assets;
mod guard;
mod ollama_proxy;
mod routes;

use std::sync::{Arc, mpsc};

use axum::{Router, extract::DefaultBodyLimit, middleware};
use tokio::sync::{OnceCell, RwLock};

use crate::config::{Paths, Settings};
use crate::ollama::Ollama;
use crate::storage::ConversationStore;
use crate::system::SystemInfo;

/// Shared state available to every handler.
#[derive(Clone)]
pub struct AppState {
    pub paths: Paths,
    pub settings: Arc<RwLock<Settings>>,
    pub store: ConversationStore,
    pub ollama: Ollama,
    /// Hardware summary, probed once on first use.
    pub system: Arc<OnceCell<SystemInfo>>,
}

impl AppState {
    pub async fn ollama_url(&self) -> String {
        self.settings.read().await.ollama_url.clone()
    }

    /// Hardware summary; the (slow, blocking) probe runs only once.
    pub async fn system_info(&self) -> SystemInfo {
        self.system
            .get_or_init(|| async {
                tokio::task::spawn_blocking(crate::system::probe).await.unwrap_or_default()
            })
            .await
            .clone()
    }
}

/// Run the server on `port` (0 = any free port); the bound port is sent on `port_tx`.
pub async fn run(paths: Paths, port: u16, port_tx: mpsc::Sender<u16>) -> anyhow::Result<()> {
    let state = AppState {
        settings: Arc::new(RwLock::new(Settings::load(&paths.settings_file))),
        store: ConversationStore::new(&paths.conversations_dir),
        ollama: Ollama::new(),
        system: Arc::new(OnceCell::new()),
        paths,
    };

    // Start Ollama in the background so it is usually ready by the time the UI asks.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let url = state.ollama_url().await;
            state.ollama.start(&url).await;
        });
    }
    // Warm the hardware probe so the model manager opens instantly.
    {
        let state = state.clone();
        tokio::spawn(async move { state.system_info().await; });
    }

    // Port 0 = let Windows pick a free port; bound to loopback only.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let port = listener.local_addr()?.port();

    let app = Router::new()
        .nest("/api/app", routes::router())
        .nest("/api/ollama", ollama_proxy::router())
        .fallback(assets::serve)
        // Chats resend attachments with the whole history; the 2 MB default is too small.
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        // Reject requests from other websites / DNS rebinding (see guard.rs).
        .layer(middleware::from_fn_with_state(guard::BoundPort(port), guard::check))
        .with_state(state);

    port_tx.send(port)?;
    axum::serve(listener, app).await?;
    Ok(())
}
