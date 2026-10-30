//! Store tests: assistant.json round trip, id validation, and the full
//! add → search → remove → reindex flow against a fake `/api/embed` server.

use super::*;
use axum::{Json, Router, http::StatusCode, routing::post};
use serde_json::{Value, json};

fn temp_store(http: reqwest::Client) -> (AssistantStore, PathBuf) {
    let dir = std::env::temp_dir().join(format!("lac-assistants-test-{}", new_id()));
    std::fs::create_dir_all(&dir).unwrap();
    (AssistantStore::new(&dir, http), dir)
}

/// Deterministic bag-of-keywords embedding: each folded keyword hashes into
/// one of 64 buckets. Model `"missing"` answers like Ollama does for an
/// uninstalled model.
async fn fake_embed(Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
    let model = body["model"].as_str().unwrap_or_default();
    if model == "missing" {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "model \"missing\" not found, try pulling it first" })),
        );
    }
    let dim = if model == "other" { 32 } else { 64 };
    let embeddings: Vec<Vec<f32>> = body["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            let mut v = vec![0.0f32; dim];
            for kw in index::keywords(t.as_str().unwrap()) {
                let h = kw
                    .bytes()
                    .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
                v[h as usize % dim] += 1.0;
            }
            v
        })
        .collect();
    (
        StatusCode::OK,
        Json(json!({ "model": model, "embeddings": embeddings })),
    )
}

async fn fake_ollama() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/api/embed", post(fake_embed)),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}")
}

#[test]
fn id_validation() {
    assert!(AssistantStore::valid_id("abc-DEF_123"));
    assert!(!AssistantStore::valid_id(""));
    assert!(!AssistantStore::valid_id("../etc"));
    assert!(!AssistantStore::valid_id("a/b"));
    assert!(!AssistantStore::valid_id("a\\b"));
    assert!(!AssistantStore::valid_id("مساعد"));
    assert!(!AssistantStore::valid_id(&"x".repeat(65)));
    let (store, dir) = temp_store(reqwest::Client::new());
    assert!(store.get("../x").unwrap().is_none());
    assert!(store.doc_text("ok", "../../secret").unwrap().is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn assistant_json_round_trip_keeps_unknown_fields() {
    let src = json!({
        "id": "a1", "name": "مساعد", "future_field": { "x": [1, 2] },
        "docs": [{ "id": "d1", "name": "n.pdf", "kind": "file", "size": 10, "chunks": 2, "added_at": 5, "pinned": true }]
    });
    let a: Assistant = serde_json::from_value(src).unwrap();
    // Defaults for missing fields.
    assert!(a.strict);
    assert_eq!(a.embed_model, DEFAULT_EMBED_MODEL);
    assert_eq!(a.docs[0].kind, DocKind::File);
    let out = serde_json::to_value(&a).unwrap();
    assert_eq!(out["future_field"], json!({ "x": [1, 2] }));
    assert_eq!(out["docs"][0]["pinned"], json!(true));
    assert_eq!(out["name"], json!("مساعد"));
    let back: Assistant = serde_json::from_value(out).unwrap();
    assert_eq!(back, a);
}

#[test]
fn patch_distinguishes_null_and_absent_model() {
    let p: AssistantPatch = serde_json::from_value(json!({ "name": "x" })).unwrap();
    assert_eq!(p.model, None);
    let p: AssistantPatch = serde_json::from_value(json!({ "model": null })).unwrap();
    assert_eq!(p.model, Some(None));
    let p: AssistantPatch = serde_json::from_value(json!({ "model": "qwen3" })).unwrap();
    assert_eq!(p.model, Some(Some("qwen3".into())));
}

#[tokio::test]
async fn crud_and_list_order() {
    let (store, dir) = temp_store(reqwest::Client::new());
    assert!(
        store
            .create(NewAssistant {
                name: "  ".into(),
                ..Default::default()
            })
            .is_err()
    );
    let a = store
        .create(NewAssistant {
            name: "First".into(),
            ..Default::default()
        })
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let b = store
        .create(NewAssistant {
            name: "Second".into(),
            strict: Some(false),
            ..Default::default()
        })
        .unwrap();
    assert!(!b.strict && a.strict);
    let names: Vec<_> = store.list().unwrap().into_iter().map(|x| x.name).collect();
    assert_eq!(names, ["Second", "First"]);

    std::thread::sleep(std::time::Duration::from_millis(5));
    let patch = AssistantPatch {
        instructions: Some("كن مختصرا".into()),
        ..Default::default()
    };
    let a2 = store.update(&a.id, patch).await.unwrap();
    assert_eq!(a2.instructions, "كن مختصرا");
    assert_eq!(store.list().unwrap()[0].id, a.id);

    let err = store
        .update("nope", AssistantPatch::default())
        .await
        .unwrap_err();
    assert!(err.downcast_ref::<NotFound>().is_some());
    store.delete(&a.id).await.unwrap();
    store.delete(&a.id).await.unwrap(); // idempotent
    assert!(store.get(&a.id).unwrap().is_none());
    assert_eq!(store.list().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn add_search_remove_reindex() {
    let url = fake_ollama().await;
    let (store, dir) = temp_store(reqwest::Client::new());
    let a = store
        .create(NewAssistant {
            name: "KB".into(),
            ..Default::default()
        })
        .unwrap();

    let mut events = Vec::new();
    let note = store
        .add_document(
            &url,
            &a.id,
            "notes",
            DocInput::Note("عاصمة فرنسا هي باريس.\n\nThe secret code is ZX-4471.".into()),
            |p| events.push(p),
        )
        .await
        .unwrap();
    assert_eq!(note.kind, DocKind::Note);
    assert_eq!(note.chunks, 1);
    assert!(
        events
            .iter()
            .any(|p| p.status == "embedding" && p.done == 1 && p.total == 1)
    );

    let long = "Bananas are yellow fruit rich in potassium. ".repeat(60);
    let file = store
        .add_document(
            &url,
            &a.id,
            "fruit.txt",
            DocInput::File(long.into_bytes()),
            |_| {},
        )
        .await
        .unwrap();
    assert!(file.chunks > 1);
    assert!(
        store
            .add_document(&url, &a.id, "x.png", DocInput::File(vec![1, 2]), |_| {})
            .await
            .is_err()
    );
    assert!(
        store
            .add_document(&url, &a.id, "empty", DocInput::Note("  \n ".into()), |_| {})
            .await
            .is_err()
    );

    let r = store
        .search(&url, &a.id, "ما هي عاصمة فرنسا؟", None)
        .await
        .unwrap();
    assert_eq!(r[0].doc_name, "notes");
    assert!(r[0].text.contains("باريس"));
    let r = store
        .search(&url, &a.id, "bananas potassium", Some(2))
        .await
        .unwrap();
    assert!(r.len() <= 2 && r[0].doc_id == file.id);
    assert!(
        store
            .search(&url, &a.id, "", None)
            .await
            .unwrap()
            .is_empty()
    );

    // Persistence: a fresh store (empty cache) sees the same data.
    let store2 = AssistantStore::new(&dir, reqwest::Client::new());
    let st = store2.index_status(&a.id).await.unwrap();
    assert_eq!(st.chunks, 1 + file.chunks);
    assert!(!st.stale);
    assert_eq!(
        store2.doc_text(&a.id, &note.id).unwrap().unwrap(),
        "عاصمة فرنسا هي باريس.\n\nThe secret code is ZX-4471."
    );

    // Remove a doc: its chunks go away.
    store.remove_document(&a.id, &file.id).await.unwrap();
    assert!(
        store
            .remove_document(&a.id, &file.id)
            .await
            .unwrap_err()
            .downcast_ref::<NotFound>()
            .is_some()
    );
    assert_eq!(store.index_status(&a.id).await.unwrap().chunks, 1);
    assert!(
        store
            .search(&url, &a.id, "bananas potassium", None)
            .await
            .unwrap()
            .is_empty()
    );

    // Change embed model → stale → search refuses; reindex fixes it.
    store
        .update(
            &a.id,
            AssistantPatch {
                embed_model: Some("other".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(store.index_status(&a.id).await.unwrap().stale);
    assert!(store.search(&url, &a.id, "فرنسا", None).await.is_err());
    let re = store.reindex(&url, &a.id, |_| {}).await.unwrap();
    assert_eq!(re.docs[0].chunks, 1);
    let st = store.index_status(&a.id).await.unwrap();
    assert!(!st.stale && st.dim == 32 && st.embed_model == "other");
    assert!(
        !store
            .search(&url, &a.id, "عاصمة فرنسا", None)
            .await
            .unwrap()
            .is_empty()
    );

    // Missing embed model → typed error, nothing recorded.
    store
        .update(
            &a.id,
            AssistantPatch {
                embed_model: Some("missing".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let err = store
        .add_document(
            &url,
            &a.id,
            "n2",
            DocInput::Note("hello there".into()),
            |_| {},
        )
        .await
        .unwrap_err();
    assert_eq!(
        err.downcast_ref::<ModelMissing>(),
        Some(&ModelMissing("missing".into()))
    );
    assert_eq!(
        err.to_string(),
        "embedding model 'missing' is not installed"
    );
    let after = store.get(&a.id).unwrap().unwrap();
    assert_eq!(after.docs.len(), 1);
    assert_eq!(
        std::fs::read_dir(dir.join(&a.id).join("docs"))
            .unwrap()
            .count(),
        1
    );

    let _ = std::fs::remove_dir_all(dir);
}
