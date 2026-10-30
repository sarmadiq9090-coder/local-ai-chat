//! Export a conversation as Markdown into `Documents\LocalAiChat\`. See `docs/API.md`.

use axum::{Json, Router, extract::State, routing::post};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::write_atomic;
use crate::error::ApiResult;
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/export", post(export))
}

#[derive(Deserialize)]
struct ExportReq {
    filename: String,
    content: String,
}

async fn export(State(s): State<AppState>, Json(req): Json<ExportReq>) -> ApiResult<Json<Value>> {
    let name = sanitize_filename(&req.filename);
    std::fs::create_dir_all(&s.paths.export_dir)?;
    let path = unique_path(&s.paths.export_dir, &name);
    write_atomic(&path, req.content.as_bytes())?;
    reveal_in_explorer(&path);
    Ok(Json(json!({ "path": path })))
}

/// Keep letters (incl. Arabic), digits, spaces, `-_.`; ensure a `.md` extension.
fn sanitize_filename(raw: &str) -> String {
    let mut name: String = raw
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .chars()
        .take(80)
        .collect();
    if name.is_empty() {
        name = "conversation".into();
    }
    if !name.to_lowercase().ends_with(".md") {
        name.push_str(".md");
    }
    name
}

/// `dir/name`, or `dir/<stem> (2).md`, `(3)`… if that file already exists,
/// so an export never overwrites an earlier one.
fn unique_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    // `sanitize_filename` guarantees a `.md` extension (any case).
    let (stem, ext) = if name.to_ascii_lowercase().ends_with(".md") {
        name.split_at(name.len() - 3)
    } else {
        (name, "")
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("some free name")
}

fn reveal_in_explorer(path: &std::path::Path) {
    let mut cmd = std::process::Command::new("explorer");
    // explorer doesn't understand a fully quoted `"/select,..."` argument, so
    // pass it raw with only the path quoted (paths often contain spaces).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.raw_arg(format!("/select,\"{}\"", path.display()));
    }
    #[cfg(not(windows))]
    cmd.arg(path);
    let _ = cmd.spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_adds_md_extension() {
        assert_eq!(sanitize_filename("chat"), "chat.md");
        assert_eq!(sanitize_filename("a/b:c.md"), "a_b_c.md");
        assert_eq!(sanitize_filename("..."), "conversation.md");
    }

    #[test]
    fn existing_exports_are_not_overwritten() {
        let dir = std::env::temp_dir().join(format!("lac-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        assert_eq!(unique_path(&dir, "محادثة.md"), dir.join("محادثة.md"));
        std::fs::write(dir.join("محادثة.md"), "1").unwrap();
        assert_eq!(unique_path(&dir, "محادثة.md"), dir.join("محادثة (2).md"));
        std::fs::write(dir.join("محادثة (2).md"), "2").unwrap();
        assert_eq!(unique_path(&dir, "محادثة.md"), dir.join("محادثة (3).md"));

        std::fs::write(dir.join("Notes.MD"), "x").unwrap();
        assert_eq!(unique_path(&dir, "Notes.MD"), dir.join("Notes (2).MD"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
