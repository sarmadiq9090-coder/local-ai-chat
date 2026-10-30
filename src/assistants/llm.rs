//! Minimal Ollama chat client for the assistants' background work (document
//! digests, query rewriting): `POST {ollama_url}/api/chat`, streamed so a
//! cancel (or a closed client) can stop generation between tokens.
//!
//! Thinking is always off here (`think: false`, accepted by every model) and
//! any `<think>…</think>` block a model still emits is removed.

use std::time::Duration;

use anyhow::{Context, bail};
use futures_util::StreamExt;
use serde_json::{Value, json};

/// The chat model isn't installed in Ollama.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatModelMissing(pub String);

impl std::fmt::Display for ChatModelMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chat model '{}' is not installed", self.0)
    }
}

impl std::error::Error for ChatModelMissing {}

/// Generation was cancelled by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// One non-thinking chat completion.
#[derive(Clone, Debug)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub system: &'a str,
    pub user: &'a str,
    pub num_ctx: u32,
    pub num_predict: i32,
    pub temperature: f32,
    /// Whole-request timeout.
    pub timeout: Duration,
}

/// Run `req` and return the reply text (trimmed, without `<think>` blocks).
/// `cancelled()` is polled between streamed tokens; when it turns true the
/// request is dropped (Ollama stops generating) and `Err(Cancelled)` returned.
pub async fn chat(
    http: &reqwest::Client,
    base_url: &str,
    req: &ChatRequest<'_>,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> anyhow::Result<String> {
    let mut messages = Vec::new();
    if !req.system.is_empty() {
        messages.push(json!({ "role": "system", "content": req.system }));
    }
    messages.push(json!({ "role": "user", "content": req.user }));
    let body = json!({
        "model": req.model,
        "messages": messages,
        "stream": true,
        "think": false,
        "options": {
            "temperature": req.temperature,
            "num_ctx": req.num_ctx,
            "num_predict": req.num_predict,
        },
    });
    let resp = http
        .post(format!("{}/api/chat", base_url.trim_end_matches('/')))
        .json(&body)
        .timeout(req.timeout)
        .send()
        .await
        .with_context(|| format!("Ollama is not reachable at {base_url}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(text);
        if status == reqwest::StatusCode::NOT_FOUND || msg.contains("not found") {
            return Err(ChatModelMissing(req.model.to_string()).into());
        }
        bail!("chat failed ({status}): {msg}");
    }
    let mut out = String::new();
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(bytes) = stream.next().await {
        if cancelled() {
            return Err(Cancelled.into());
        }
        buf.extend_from_slice(&bytes.context("chat stream interrupted")?);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            if take_line(&line, &mut out)? == Some(true) {
                return Ok(clean_reply(&out));
            }
        }
    }
    if !buf.is_empty() {
        take_line(&buf, &mut out)?;
    }
    if cancelled() {
        return Err(Cancelled.into());
    }
    Ok(clean_reply(&out))
}

/// Parse one NDJSON line; appends content and returns `Some(done)`.
fn take_line(line: &[u8], out: &mut String) -> anyhow::Result<Option<bool>> {
    let line = std::str::from_utf8(line).unwrap_or_default().trim();
    if line.is_empty() {
        return Ok(None);
    }
    let v: Value = serde_json::from_str(line).context("invalid chat stream line")?;
    if let Some(e) = v.get("error").and_then(Value::as_str) {
        bail!("chat failed: {e}");
    }
    if let Some(c) = v.pointer("/message/content").and_then(Value::as_str) {
        out.push_str(c);
    }
    Ok(Some(
        v.get("done").and_then(Value::as_bool).unwrap_or(false),
    ))
}

/// Remove `<think>…</think>` blocks (an unterminated one up to the end) and
/// surrounding whitespace.
pub fn clean_reply(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn think_blocks_are_removed() {
        assert_eq!(clean_reply("<think>hmm</think>\n\nAnswer"), "Answer");
        assert_eq!(clean_reply("A <think>x</think>B"), "A B");
        assert_eq!(clean_reply("Answer<think>unfinished"), "Answer");
        assert_eq!(clean_reply("  plain  "), "plain");
    }

    #[test]
    fn stream_lines_accumulate() {
        let mut out = String::new();
        assert_eq!(
            take_line(br#"{"message":{"content":"Hel"},"done":false}"#, &mut out).unwrap(),
            Some(false)
        );
        assert_eq!(
            take_line(br#"{"message":{"content":"lo"},"done":true}"#, &mut out).unwrap(),
            Some(true)
        );
        assert_eq!(out, "Hello");
        assert!(take_line(br#"{"error":"boom"}"#, &mut out).is_err());
        assert_eq!(take_line(b"  ", &mut out).unwrap(), None);
    }
}
