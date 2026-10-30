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

/// Streaming `/api/chat`: a rewrite prompt gets a standalone query, anything
/// else a small Markdown digest. Model `"missing"` → 404 like Ollama.
async fn fake_chat(Json(body): Json<Value>) -> axum::response::Response {
    use axum::response::IntoResponse;
    let model = body["model"].as_str().unwrap_or_default();
    if model == "missing" {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "model \"missing\" not found, try pulling it first" })),
        )
            .into_response();
    }
    assert_eq!(body["think"], json!(false));
    let system = body["messages"][0]["content"].as_str().unwrap_or_default();
    let reply = if system.contains("standalone search query") {
        "<think>x</think>Query: \"housing allowance for managers\"".to_string()
    } else {
        "## Overview\nThe handbook explains housing allowance and leave.\n\n## Key facts and numbers\n- Housing allowance: 25% of salary".to_string()
    };
    let half = reply.len() / 2;
    let half = (0..=half)
        .rev()
        .find(|&i| reply.is_char_boundary(i))
        .unwrap();
    let lines = format!(
        "{}\n{}\n",
        json!({ "message": { "content": &reply[..half] }, "done": false }),
        json!({ "message": { "content": &reply[half..] }, "done": true })
    );
    (
        [(axum::http::header::CONTENT_TYPE, "application/x-ndjson")],
        lines,
    )
        .into_response()
}

async fn fake_ollama() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/api/embed", post(fake_embed))
                .route("/api/chat", post(fake_chat)),
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
    let names: Vec<_> = store
        .list()
        .unwrap()
        .assistants
        .into_iter()
        .map(|x| x.name)
        .collect();
    assert_eq!(names, ["Second", "First"]);

    std::thread::sleep(std::time::Duration::from_millis(5));
    let patch = AssistantPatch {
        instructions: Some("كن مختصرا".into()),
        ..Default::default()
    };
    let a2 = store.update(&a.id, patch).await.unwrap();
    assert_eq!(a2.instructions, "كن مختصرا");
    assert_eq!(store.list().unwrap().assistants[0].id, a.id);

    let err = store
        .update("nope", AssistantPatch::default())
        .await
        .unwrap_err();
    assert!(err.downcast_ref::<NotFound>().is_some());
    store.delete(&a.id).await.unwrap();
    store.delete(&a.id).await.unwrap(); // idempotent
    assert!(store.get(&a.id).unwrap().is_none());
    assert_eq!(store.list().unwrap().assistants.len(), 1);
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
    let err = store.search(&url, &a.id, "فرنسا", None).await.unwrap_err();
    assert!(err.downcast_ref::<ReindexRequired>().is_some(), "{err:#}");
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

#[test]
fn size_limit_helpers() {
    assert!(check_doc_text_size("a.txt", MAX_DOC_TEXT_BYTES).is_ok());
    let e = check_doc_text_size("a.txt", MAX_DOC_TEXT_BYTES + 1).unwrap_err();
    assert!(e.to_string().contains("document too large"), "{e}");
    assert!(check_chunk_total(MAX_CHUNKS).is_ok());
    let e = check_chunk_total(MAX_CHUNKS + 1).unwrap_err();
    assert!(e.to_string().contains("knowledge base too large"), "{e}");
}

/// Both caps reject before any embedding call: the Ollama URL is
/// unreachable, so reaching `/api/embed` would fail with a different error.
#[tokio::test]
async fn oversized_doc_and_knowledge_base_are_rejected_before_embedding() {
    let dead = "http://127.0.0.1:9"; // discard port: nothing listens
    let (store, dir) = temp_store(reqwest::Client::new());
    let a = store
        .create(NewAssistant {
            name: "Big".into(),
            ..Default::default()
        })
        .unwrap();

    // 1) One document with more than 5 MB of text.
    let huge = "word ".repeat(MAX_DOC_TEXT_BYTES / 5 + 10);
    let err = store
        .add_document(dead, &a.id, "huge", DocInput::Note(huge), |_| {})
        .await
        .unwrap_err();
    assert!(err.downcast_ref::<TooLarge>().is_some(), "{err:#}");
    assert!(store.get(&a.id).unwrap().unwrap().docs.is_empty());
    let docs_dir = dir.join(&a.id).join("docs");
    assert_eq!(std::fs::read_dir(&docs_dir).unwrap().count(), 0);

    // 2) A knowledge base already at the chunk cap.
    let full = Index {
        version: index::INDEX_VERSION,
        embed_model: a.embed_model.clone(),
        dim: 0,
        chunks: (0..MAX_CHUNKS)
            .map(|i| IndexedChunk {
                doc_id: "old".into(),
                i,
                kind: None,
                text: String::new(),
                vec: Vec::new(),
            })
            .collect(),
    };
    store.save_index(&a.id, full).await.unwrap();
    let err = store
        .add_document(
            dead,
            &a.id,
            "one more",
            DocInput::Note("small note".into()),
            |_| {},
        )
        .await
        .unwrap_err();
    assert!(err.downcast_ref::<TooLarge>().is_some(), "{err:#}");
    assert!(store.get(&a.id).unwrap().unwrap().docs.is_empty());
    assert_eq!(std::fs::read_dir(&docs_dir).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

/// Retrieval eval against the real `bge-m3` (needs a running Ollama with it pulled):
/// `cargo test retrieval_eval_live -- --ignored --nocapture`.
/// Prints, per question, the best combined score (cosine + keyword boost) among
/// the relevant chunks and among the others; the chosen `index::MIN_SCORE` and
/// `index::RELATIVE_WINDOW` must keep every relevant hit and drop every
/// unrelated question. Results are recorded in `index.rs` and the spec.
#[tokio::test]
#[ignore]
async fn retrieval_eval_live() {
    let url = std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into());
    let http = reqwest::Client::new();
    let docs: &[(&str, &str)] = &[
        (
            "faq_ar",
            "س: ما هي سياسة الإرجاع؟\n\nج: يمكن إرجاع المنتجات خلال 14 يومًا من تاريخ الشراء مع الفاتورة، بشرط أن تكون غير مستعملة وفي علبتها الأصلية. لا تُقبل إعادة المنتجات المستعملة.\n\n\
             س: كم تكلفة الشحن؟\n\nج: الشحن داخل بغداد خلال يوم واحد بتكلفة 5,000 دينار، وإلى باقي المحافظات خلال ثلاثة أيام بتكلفة 8,000 دينار.\n\n\
             س: ما هي أوقات الدوام؟\n\nج: نعمل يوميًا من التاسعة صباحًا حتى التاسعة مساءً ما عدا يوم الجمعة حيث نفتح من الثانية ظهرًا.",
        ),
        (
            "handbook_en",
            "# Employee handbook\n\n## Transport allowance\n\nEvery full-time employee receives a monthly transport allowance of 75,000 Iraqi dinars, paid with the salary at the end of each month.\n\n\
             ## Annual leave\n\nEmployees are entitled to 21 days of paid annual leave per year. Leave requests must be submitted to the manager two weeks in advance.\n\n\
             ## Working from home\n\nRemote work is allowed up to two days per week after the probation period of three months, with the manager's approval.",
        ),
        (
            "policy_ar",
            "سياسة الموظفين: يبدأ الدوام الرسمي في الساعة الثامنة صباحًا وينتهي في الرابعة عصرًا من الأحد إلى الخميس.              تُحتسب ساعات العمل الإضافية بأجر ونصف عن كل ساعة بعد موافقة المدير المباشر. يحق للموظف الحصول على عشرة أيام إجازة مرضية سنويًا بشرط تقديم تقرير طبي معتمد.              يلتزم الموظفون بالزي الرسمي طوال أيام الأسبوع، ويُسمح بالملابس غير الرسمية يوم الخميس فقط.              يتوفر موقف سيارات مجاني للموظفين في الطابق السفلي من المبنى، وتُصرف بطاقة الدخول من قسم الموارد البشرية.",
        ),
        (
            "cooking_en",
            "To make hummus, blend two cups of cooked chickpeas with tahini, lemon juice, garlic and a little salt until smooth, then drizzle olive oil on top.",
        ),
    ];
    // (question, Some(substring of the relevant chunk) | None = unrelated)
    let qs: &[(&str, Option<&str>)] = &[
        ("ما هي سياسة الإرجاع؟", Some("14 يومًا")),
        ("هل يمكن إرجاع منتج مستعمل؟", Some("14 يومًا")),
        ("كم تكلفة الشحن إلى البصرة؟", Some("8,000")),
        ("متى تفتحون يوم الجمعة؟", Some("الجمعة")),
        ("What is the return policy?", Some("14 يومًا")),
        ("How much does shipping cost?", Some("8,000")),
        ("What are your opening hours?", Some("الجمعة")),
        ("كم بدل النقل الشهري للموظف؟", Some("75,000")),
        ("كم يوم إجازة سنوية يحصل عليها الموظف؟", Some("21 days")),
        ("هل يمكنني العمل من المنزل؟", Some("Remote work")),
        (
            "How much is the monthly transport allowance?",
            Some("75,000"),
        ),
        ("How many days of annual leave do I get?", Some("21 days")),
        ("How do I make hummus?", Some("hummus")),
        ("What is the overtime pay rate?", Some("بأجر ونصف")),
        ("Is there a dress code?", Some("بأجر ونصف")),
        ("هل يوجد موقف سيارات للموظفين؟", Some("بأجر ونصف")),
        ("How many sick days are allowed?", Some("بأجر ونصف")),
        ("Can I park my car at the office?", Some("بأجر ونصف")),
        ("What is the capital of France?", None),
        ("ما هي عاصمة فرنسا؟", None),
        ("من فاز بكأس العالم 2018؟", None),
        ("How do I bake a chocolate cake?", None),
        ("ما هو سعر صرف الدولار اليوم؟", None),
        ("Who is the CEO of the company?", None),
        ("اكتب لي قصيدة عن البحر", None),
    ];
    let mut chunks: Vec<String> = Vec::new();
    for (_, text) in docs {
        chunks.extend(chunk::chunk(text));
    }
    let cvecs = embed::embed(&http, &url, "bge-m3", &chunks)
        .await
        .expect("embed chunks");
    let questions: Vec<String> = qs.iter().map(|q| q.0.to_string()).collect();
    let qvecs = embed::embed(&http, &url, "bge-m3", &questions)
        .await
        .expect("embed questions");
    println!("{} chunks", chunks.len());
    let mut min_rel = f32::MAX;
    let mut max_irr = f32::MIN;
    for ((q, want), qv) in qs.iter().zip(&qvecs) {
        let kw = index::keywords(q);
        let scored: Vec<(f32, f32, &String)> = chunks
            .iter()
            .zip(&cvecs)
            .map(|(c, v)| {
                let cos = index::dot(v, qv);
                (cos, cos + index::keyword_boost(&kw, c), c)
            })
            .collect();
        let best = |rel: bool| {
            scored
                .iter()
                .filter(|s| want.is_some_and(|w| s.2.contains(w)) == rel)
                .map(|s| (s.0, s.1))
                .fold((f32::MIN, f32::MIN), |a, b| (a.0.max(b.0), a.1.max(b.1)))
        };
        let (rc, rs) = best(true);
        let (ic, is) = best(false);
        if want.is_some() {
            min_rel = min_rel.min(rs);
        }
        max_irr = max_irr.max(is);
        let kept = index::top_k_from(
            scored.iter().enumerate().map(|(i, s)| (i, s.1)).collect(),
            5,
            index::MIN_SCORE,
        );
        let verdict = match want {
            Some(w) if kept.iter().any(|&(i, _)| chunks[i].contains(w)) => "found",
            Some(_) => "MISSED",
            None if kept.is_empty() => "dropped",
            // Near-miss: passed on to the model, which must still answer
            // "not in knowledge" in strict mode (the UI then hides the sources).
            None => "near-miss",
        };
        let relevant = if want.is_some() {
            format!("cos {rc:.3} / score {rs:.3}")
        } else {
            "–".into()
        };
        println!(
            "{verdict:9} | relevant {relevant} | others cos {ic:.3} / score {is:.3} | kept {} | {q}",
            kept.len(),
        );
        assert_ne!(verdict, "MISSED", "{q}");
        if want.is_none() && is < 0.40 {
            assert_eq!(verdict, "dropped", "{q}");
        }
    }
    println!("min relevant score {min_rel:.3}, max other score {max_irr:.3}");
}

/// An index written by the old chunker (no `version`) is stale: search asks
/// for a re-index, and re-indexing rebuilds it with the current version.
#[tokio::test]
async fn old_index_version_requires_reindex() {
    let url = fake_ollama().await;
    let (store, dir) = temp_store(reqwest::Client::new());
    let a = store
        .create(NewAssistant {
            name: "Old".into(),
            ..Default::default()
        })
        .unwrap();
    let doc = store
        .add_document(
            &url,
            &a.id,
            "faq",
            DocInput::Note("Returns are accepted within 14 days with the receipt.".into()),
            |_| {},
        )
        .await
        .unwrap();
    // Rewrite index.json the way v2.1.0 wrote it (no "version" field).
    let path = dir.join(&a.id).join("index.json");
    let mut v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    v.as_object_mut().unwrap().remove("version");
    std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();

    let store2 = AssistantStore::new(&dir, reqwest::Client::new());
    assert!(store2.index_status(&a.id).await.unwrap().stale);
    let err = store2
        .search(&url, &a.id, "returns", None)
        .await
        .unwrap_err();
    assert!(err.downcast_ref::<ReindexRequired>().is_some(), "{err:#}");
    let re = store2.reindex(&url, &a.id, |_| {}).await.unwrap();
    assert_eq!(re.docs[0].id, doc.id);
    assert!(!store2.index_status(&a.id).await.unwrap().stale);
    let r = store2
        .search(&url, &a.id, "returns 14 days", None)
        .await
        .unwrap();
    assert_eq!(r[0].doc_id, doc.id);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn corrupt_assistant_json_is_backed_up_and_skipped() {
    let (store, dir) = temp_store(reqwest::Client::new());
    let good = store
        .create(NewAssistant {
            name: "Good".into(),
            ..Default::default()
        })
        .unwrap();
    let bad = store
        .create(NewAssistant {
            name: "Bad".into(),
            ..Default::default()
        })
        .unwrap();
    let bad_file = dir.join(&bad.id).join("assistant.json");
    std::fs::write(&bad_file, b"{ \"name\": \"Bad\", oops").unwrap();

    let list = store.list().unwrap();
    assert_eq!(list.assistants.len(), 1);
    assert_eq!(list.assistants[0].id, good.id);
    assert_eq!(list.corrupt, vec![bad.id.clone()]);
    assert!(!bad_file.exists());
    let backups: Vec<_> = std::fs::read_dir(dir.join(&bad.id))
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("assistant.json.corrupt-"))
        .collect();
    assert_eq!(backups.len(), 1);
    // Next time it is simply gone (no repeated warning).
    let list = store.list().unwrap();
    assert_eq!(list.assistants.len(), 1);
    assert!(list.corrupt.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn embed_model_installed_check() {
    async fn tags() -> Json<Value> {
        Json(json!({ "models": [{ "name": "bge-m3:latest" }, { "name": "qwen3:8b" }] }))
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/api/tags", axum::routing::get(tags)),
        )
        .await
        .unwrap();
    });
    let http = reqwest::Client::new();
    assert_eq!(embed::is_installed(&http, &url, "bge-m3").await, Some(true));
    assert_eq!(
        embed::is_installed(&http, &url, "bge-m3:latest").await,
        Some(true)
    );
    assert_eq!(
        embed::is_installed(&http, &url, "nomic-embed-text").await,
        Some(false)
    );
    // Unreachable Ollama: unknown, not "missing".
    assert_eq!(
        embed::is_installed(&http, "http://127.0.0.1:9", "bge-m3").await,
        None
    );

    let (store, dir) = temp_store(http);
    let a = store
        .create(NewAssistant {
            name: "E".into(),
            embed_model: Some("nomic-embed-text".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        store.missing_embed_model(&url, &a.id).await.as_deref(),
        Some("nomic-embed-text")
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn latest_tag_is_the_same_embed_model() {
    assert!(same_model("bge-m3", "bge-m3:latest"));
    assert!(same_model(" bge-m3:latest", "bge-m3"));
    assert!(!same_model("bge-m3", "bge-m3:567m"));
    let a = Assistant {
        embed_model: "bge-m3".into(),
        ..Default::default()
    };
    let idx = Index {
        version: index::INDEX_VERSION,
        embed_model: "bge-m3:latest".into(),
        dim: 1,
        chunks: vec![IndexedChunk {
            doc_id: "x".into(),
            ..Default::default()
        }],
    };
    assert!(!is_stale(&a, &idx));
    let other = Index {
        embed_model: "nomic-embed-text".into(),
        ..idx
    };
    assert!(is_stale(&a, &other));
}

#[tokio::test]
async fn deep_mode_digest_context_and_cleanup() {
    use super::deep::{AppDefaults, ChatTurn, ContextRequest, KIND_DIGEST};
    let url = fake_ollama().await;
    let (store, dir) = temp_store(reqwest::Client::new());
    let a = store
        .create(NewAssistant {
            name: "HR".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(a.deep, "new assistants get deep understanding");
    let text = (1..=12)
        .map(|i| format!("## Section {i}\nRule {i}: housing allowance grade {i} is {} dollars per month for staff.", i * 100))
        .collect::<Vec<_>>()
        .join("\n\n");
    let doc = store
        .add_document(
            &url,
            &a.id,
            "handbook.md",
            DocInput::File(text.into_bytes()),
            |_| {},
        )
        .await
        .unwrap();
    assert!(doc.chunks >= 2);
    assert!(doc.chars.unwrap() > 100);

    // Digest: generated, saved, embedded; the index stays consistent.
    let never = || false;
    let job = digest::DigestJob {
        model: "m",
        num_ctx: 8192,
        cancelled: &never,
    };
    let mut events = Vec::new();
    let mut results = Vec::new();
    let a2 = store
        .digest_documents(
            &url,
            &a.id,
            None,
            false,
            &job,
            |p| events.push(p),
            |d, _, r| {
                results.push((
                    d.to_string(),
                    r.map(|m| m.status).map_err(|e| e.to_string()),
                ))
            },
        )
        .await
        .unwrap();
    assert_eq!(results, vec![(doc.id.clone(), Ok("ok".to_string()))]);
    assert!(
        events
            .iter()
            .any(|p| p.status == "digesting" && p.done == 1)
    );
    let meta = a2.docs[0].digest.clone().unwrap();
    assert!(meta.chunks >= 1 && meta.chars > 20 && meta.model == "m");
    assert!(
        store
            .digest_text(&a.id, &doc.id)
            .unwrap()
            .unwrap()
            .contains("25%")
    );
    let st = store.index_status(&a.id).await.unwrap();
    assert_eq!(st.digest_chunks, meta.chunks);
    assert!(!st.stale);
    // Nothing left to do without force.
    let mut n = 0;
    store
        .digest_documents(&url, &a.id, None, false, &job, |_| {}, |_, _, _| n += 1)
        .await
        .unwrap();
    assert_eq!(n, 0);

    // /search never returns digest chunks.
    let r = store
        .search(&url, &a.id, "housing allowance overview handbook", Some(50))
        .await
        .unwrap();
    assert!(r.iter().all(|x| !x.text.contains("## Overview")));

    // Context: rewritten follow-up, full digest first, expanded passages.
    let app = AppDefaults {
        num_ctx: 8192,
        think: false,
        default_model: Some("m".into()),
    };
    let turn = |r: &str, c: &str| ChatTurn {
        role: r.into(),
        content: c.into(),
    };
    let req = ContextRequest {
        messages: vec![
            turn("user", "What is the housing allowance for grade 3?"),
            turn("assistant", "It is 300 dollars per month."),
            turn("user", "and for managers?"),
        ],
        ..Default::default()
    };
    let ctx = store.context(&url, &a.id, req.clone(), &app).await.unwrap();
    assert_eq!(ctx.rewrite, "done");
    assert_eq!(
        ctx.rewritten.as_deref(),
        Some("housing allowance for managers")
    );
    // Message, rewrite, and previous question + message.
    assert_eq!(ctx.queries.len(), 3);
    assert_eq!(
        ctx.queries[2],
        "What is the housing allowance for grade 3?
and for managers?"
    );
    assert_eq!(ctx.num_ctx, deep::DEEP_NUM_CTX);
    assert!(ctx.think);
    assert!(!ctx.passages.is_empty());
    assert_eq!(ctx.passages[0].kind, KIND_DIGEST);
    assert!(ctx.passages[0].full_digest);
    assert!(ctx.used_tokens <= ctx.budget_tokens);
    assert!(ctx.passages.iter().any(|p| p.expanded));
    // Second call: cached rewrite.
    let ctx2 = store.context(&url, &a.id, req, &app).await.unwrap();
    assert_eq!(ctx2.rewrite, "cached");

    // Edited digest replaces the chunks; reindex keeps them.
    let d = store
        .save_digest(
            &url,
            &a.id,
            &doc.id,
            "## Overview\nEdited.\n\n## Facts\n- One\n\n## More\n- Two",
        )
        .await
        .unwrap();
    assert_eq!(d.digest.as_ref().unwrap().status, "edited");
    let after = store.reindex(&url, &a.id, |_| {}).await.unwrap();
    assert_eq!(
        after.docs[0].digest.as_ref().unwrap().chunks,
        d.digest.as_ref().unwrap().chunks
    );
    assert!(!store.index_status(&a.id).await.unwrap().stale);

    // A missing chat model: the digest fails, the document stays indexed and
    // the previous digest is kept.
    let job_missing = digest::DigestJob {
        model: "missing",
        num_ctx: 8192,
        cancelled: &never,
    };
    let mut errs = Vec::new();
    store
        .digest_documents(
            &url,
            &a.id,
            Some(&doc.id),
            true,
            &job_missing,
            |_| {},
            |_, _, r| errs.push(r.is_err()),
        )
        .await
        .unwrap();
    assert_eq!(errs, vec![true]);
    let a3 = store.get(&a.id).unwrap().unwrap();
    assert_eq!(a3.docs[0].digest.as_ref().unwrap().status, "edited");
    // Cancelled before any work: nothing is digested.
    let yes = || true;
    let job_cancel = digest::DigestJob {
        model: "m",
        num_ctx: 8192,
        cancelled: &yes,
    };
    let mut calls = 0;
    store
        .digest_documents(
            &url,
            &a.id,
            None,
            true,
            &job_cancel,
            |_| {},
            |_, _, _| calls += 1,
        )
        .await
        .unwrap();
    assert_eq!(calls, 0);

    // Clearing the digest removes its chunks; deleting the doc removes files.
    store.save_digest(&url, &a.id, &doc.id, "  ").await.unwrap();
    assert_eq!(store.index_status(&a.id).await.unwrap().digest_chunks, 0);
    store
        .save_digest(&url, &a.id, &doc.id, "## X\nY")
        .await
        .unwrap();
    store.remove_document(&a.id, &doc.id).await.unwrap();
    assert!(store.digest_text(&a.id, &doc.id).unwrap().is_none());
    assert_eq!(store.index_status(&a.id).await.unwrap().chunks, 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn old_assistants_load_with_deep_off_and_digest_failure_is_recorded() {
    let url = fake_ollama().await;
    let (store, dir) = temp_store(reqwest::Client::new());
    // An assistant.json written by v2.1.1 (no deep fields).
    let id = "old1";
    std::fs::create_dir_all(dir.join(id).join("docs")).unwrap();
    std::fs::write(
        dir.join(id).join("assistant.json"),
        r#"{"id":"old1","name":"Old","strict":true,"embed_model":"bge-m3","docs":[]}"#,
    )
    .unwrap();
    let a = store.get(id).unwrap().unwrap();
    assert!(!a.deep && a.num_ctx.is_none() && a.think.is_none());
    let a = store
        .update(
            id,
            serde_json::from_value(json!({ "deep": true, "num_ctx": 999999, "think": false }))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(a.deep);
    assert_eq!(a.num_ctx, Some(deep::MAX_NUM_CTX));
    assert_eq!(a.think, Some(false));
    let a = store
        .update(
            id,
            serde_json::from_value(json!({ "num_ctx": null, "think": null })).unwrap(),
        )
        .await
        .unwrap();
    assert!(a.num_ctx.is_none() && a.think.is_none());

    let doc = store
        .add_document(
            &url,
            id,
            "n",
            DocInput::Note("Some note text about parking. ".repeat(20)),
            |_| {},
        )
        .await
        .unwrap();
    let never = || false;
    let job = digest::DigestJob {
        model: "missing",
        num_ctx: 8192,
        cancelled: &never,
    };
    let mut errs = Vec::new();
    store
        .digest_documents(
            &url,
            id,
            None,
            false,
            &job,
            |_| {},
            |_, _, r| errs.push(format!("{:#}", r.unwrap_err())),
        )
        .await
        .unwrap();
    assert!(errs[0].contains("not installed"), "{errs:?}");
    let a = store.get(id).unwrap().unwrap();
    let g = a.docs[0].digest.as_ref().unwrap();
    assert_eq!(g.status, "failed");
    assert!(g.error.as_deref().unwrap().contains("not installed"));
    // A failed digest doesn't make the index stale and search still works.
    assert!(!store.index_status(id).await.unwrap().stale);
    let r = store.search(&url, id, "parking note", None).await.unwrap();
    assert_eq!(r[0].doc_id, doc.id);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn tiny_documents_get_no_digest() {
    let url = fake_ollama().await;
    let (store, dir) = temp_store(reqwest::Client::new());
    let a = store
        .create(NewAssistant {
            name: "Tiny".into(),
            ..Default::default()
        })
        .unwrap();
    let doc = store
        .add_document(
            &url,
            &a.id,
            "n",
            DocInput::Note("الرمز البريدي هو 10011.".into()),
            |_| {},
        )
        .await
        .unwrap();
    let never = || false;
    let job = digest::DigestJob {
        model: "m",
        num_ctx: 8192,
        cancelled: &never,
    };
    let mut results = Vec::new();
    store
        .digest_documents(
            &url,
            &a.id,
            None,
            false,
            &job,
            |_| {},
            |_, _, r| results.push(r.map(|m| m.status).map_err(|e| e.to_string())),
        )
        .await
        .unwrap();
    assert_eq!(results, vec![Ok(digest::STATUS_SHORT.to_string())]);
    assert!(store.digest_text(&a.id, &doc.id).unwrap().is_none());
    let st = store.index_status(&a.id).await.unwrap();
    assert_eq!(st.digest_chunks, 0);
    assert!(!st.stale);
    // Not offered again, and a re-index keeps the "short" state.
    let mut n = 0;
    store
        .digest_documents(&url, &a.id, None, false, &job, |_| {}, |_, _, _| n += 1)
        .await
        .unwrap();
    assert_eq!(n, 0);
    let after = store.reindex(&url, &a.id, |_| {}).await.unwrap();
    assert_eq!(
        after.docs[0].digest.as_ref().unwrap().status,
        digest::STATUS_SHORT
    );
    assert!(digest::needs_digest(&"x".repeat(digest::MIN_DIGEST_CHARS)));
    assert!(!digest::needs_digest("  short  "));
    let _ = std::fs::remove_dir_all(dir);
}
