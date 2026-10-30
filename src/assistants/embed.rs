//! Ollama embeddings client: `POST {ollama_url}/api/embed`.
//!
//! Vectors are normalized to unit length here, so stored vectors can be
//! compared with a plain dot product. A missing model is reported as a typed
//! [`ModelMissing`] error (wrapped in `anyhow`) so callers can offer a pull.

use std::time::Duration;

use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json::json;

/// Inputs per `/api/embed` request.
pub const BATCH_SIZE: usize = 32;

/// The embedding model isn't installed in Ollama.
/// Detect with `err.downcast_ref::<ModelMissing>()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMissing(pub String);

impl std::fmt::Display for ModelMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "embedding model '{}' is not installed", self.0)
    }
}

impl std::error::Error for ModelMissing {}

/// Embed `inputs` in one request; returns one unit-length vector per input.
pub async fn embed(
    http: &reqwest::Client,
    base_url: &str,
    model: &str,
    inputs: &[String],
) -> anyhow::Result<Vec<Vec<f32>>> {
    #[derive(Deserialize)]
    struct Resp {
        embeddings: Vec<Vec<f32>>,
    }
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let resp = http
        .post(format!("{}/api/embed", base_url.trim_end_matches('/')))
        .json(&json!({ "model": model, "input": inputs }))
        // Generous: the first call loads the model into memory.
        .timeout(Duration::from_secs(600))
        .send()
        .await
        .with_context(|| format!("Ollama is not reachable at {base_url}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(body);
        if status == reqwest::StatusCode::NOT_FOUND || msg.contains("not found") {
            return Err(ModelMissing(model.to_string()).into());
        }
        bail!("embedding failed ({status}): {msg}");
    }

    let mut out = resp
        .json::<Resp>()
        .await
        .context("invalid /api/embed response")?
        .embeddings;
    if out.len() != inputs.len() {
        bail!(
            "embedding returned {} vectors for {} inputs",
            out.len(),
            inputs.len()
        );
    }
    let dim = out[0].len();
    if dim == 0 || out.iter().any(|v| v.len() != dim) {
        bail!("embedding returned vectors of inconsistent size");
    }
    out.iter_mut().for_each(|v| normalize(v));
    Ok(out)
}

/// [`embed`] in batches of [`BATCH_SIZE`].
pub async fn embed_batched(
    http: &reqwest::Client,
    base_url: &str,
    model: &str,
    inputs: &[String],
) -> anyhow::Result<Vec<Vec<f32>>> {
    let mut out = Vec::with_capacity(inputs.len());
    for batch in inputs.chunks(BATCH_SIZE) {
        out.extend(embed(http, base_url, model, batch).await?);
    }
    Ok(out)
}

/// Whether `model` is installed in Ollama (`GET /api/tags`; `"bge-m3"` also
/// matches `"bge-m3:latest"`). `None` if Ollama can't be asked (unreachable,
/// unexpected answer).
pub async fn is_installed(http: &reqwest::Client, base_url: &str, model: &str) -> Option<bool> {
    #[derive(Deserialize)]
    struct Tags {
        models: Vec<Tag>,
    }
    #[derive(Deserialize)]
    struct Tag {
        name: String,
    }
    let tags: Tags = http
        .get(format!("{}/api/tags", base_url.trim_end_matches('/')))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    let latest = format!("{model}:latest");
    Some(
        tags.models
            .iter()
            .any(|m| m.name == model || m.name == latest),
    )
}

/// Scale `v` to unit length (no-op for the zero vector).
pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_unit_length() {
        let mut v = vec![3.0, 4.0];
        normalize(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        let mut z = vec![0.0; 3];
        normalize(&mut z);
        assert_eq!(z, vec![0.0; 3]);
    }

    #[test]
    fn model_missing_message() {
        let e: anyhow::Error = ModelMissing("bge-m3".into()).into();
        assert_eq!(e.to_string(), "embedding model 'bge-m3' is not installed");
        assert!(e.downcast_ref::<ModelMissing>().is_some());
    }

    /// Needs a running Ollama with `bge-m3` pulled:
    /// `cargo test bge_m3_live -- --ignored --nocapture`
    /// (set `OLLAMA_URL` to override `http://127.0.0.1:11434`).
    #[tokio::test]
    #[ignore]
    async fn bge_m3_live() {
        let url = std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into());
        let http = reqwest::Client::new();
        let inputs: Vec<String> = [
            "عاصمة فرنسا هي باريس.",
            "Paris is the capital of France.",
            "The recipe needs two eggs and flour.",
        ]
        .map(String::from)
        .to_vec();
        let vecs = embed(&http, &url, "bge-m3", &inputs).await.expect("embed");
        assert_eq!(vecs.len(), 3);
        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        let same = dot(&vecs[0], &vecs[1]);
        let diff = dot(&vecs[0], &vecs[2]);
        println!("dim={} ar~en={same:.3} ar~recipe={diff:.3}", vecs[0].len());
        assert!(
            same > diff,
            "cross-lingual match should beat an unrelated sentence"
        );

        let err = embed(&http, &url, "no-such-embed-model-xyz", &inputs[..1])
            .await
            .unwrap_err();
        assert!(err.downcast_ref::<ModelMissing>().is_some(), "{err:#}");
    }
}
