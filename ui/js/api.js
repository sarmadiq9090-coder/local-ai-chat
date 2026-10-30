// api.js — every network call the UI makes, in one place.
// App endpoints live under /api/app/..., Ollama is proxied under /api/ollama/...
// (see docs/API.md). Errors are thrown as Error(message) where message is the
// backend's {"error": "..."} text when available. Streaming endpoints (chat,
// pull) are exposed as async generators yielding parsed NDJSON objects.

const JSON_HEADERS = { "Content-Type": "application/json" };

async function httpError(res) {
  let message = `HTTP ${res.status}`;
  try {
    const j = await res.json();
    if (j && j.error) message = j.error;
  } catch { /* body was not JSON */ }
  const err = new Error(message);
  err.status = res.status;
  return err;
}

async function request(method, path, body, signal) {
  const opts = { method, signal };
  if (body !== undefined) { opts.headers = JSON_HEADERS; opts.body = JSON.stringify(body); }
  const res = await fetch(path, opts);
  if (!res.ok) throw await httpError(res);
  const text = await res.text();
  return text ? JSON.parse(text) : null;
}

/** Parse an NDJSON response body into objects, one per line. */
export async function* readNdjson(res) {
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  let done = false;
  try {
    while (!done) {
      const chunk = await reader.read();
      done = chunk.done;
      buf += chunk.value ? decoder.decode(chunk.value, { stream: true }) : decoder.decode();
      let nl;
      while ((nl = buf.indexOf("\n")) >= 0) {
        const line = buf.slice(0, nl).trim();
        buf = buf.slice(nl + 1);
        if (line) yield JSON.parse(line);
      }
    }
    if (buf.trim()) yield JSON.parse(buf.trim());
  } finally {
    if (!done) { try { await reader.cancel(); } catch { /* already aborted */ } }
  }
}

async function* streamPost(path, body, signal) {
  const res = await fetch(path, { method: "POST", headers: JSON_HEADERS, body: JSON.stringify(body), signal });
  if (!res.ok) throw await httpError(res);
  for await (const obj of readNdjson(res)) {
    if (obj && obj.error) throw new Error(obj.error);
    yield obj;
  }
}

const enc = encodeURIComponent;

// ---------- app ----------
export const getStatus = () => request("GET", "/api/app/status");
export const startOllama = () => request("POST", "/api/app/ollama/start");
export const getSystemInfo = () => request("GET", "/api/app/system");
export const getSettings = () => request("GET", "/api/app/settings");
export const saveSettings = settings => request("PUT", "/api/app/settings", settings);
export const listConversations = () => request("GET", "/api/app/conversations");
export const getConversation = id => request("GET", `/api/app/conversations/${enc(id)}`);
export const saveConversation = conv => request("PUT", `/api/app/conversations/${enc(conv.id)}`, conv);
export const deleteConversation = id => request("DELETE", `/api/app/conversations/${enc(id)}`);
export const exportMarkdown = (filename, content) => request("POST", "/api/app/export", { filename, content });

// ---------- ollama ----------
export const listModels = () => request("GET", "/api/ollama/tags");
export const showModel = model => request("POST", "/api/ollama/show", { model });
export const deleteModel = model => request("DELETE", "/api/ollama/delete", { model });

/** Stream a chat completion. Yields Ollama chat chunks. */
export const chatStream = (payload, signal) => streamPost("/api/ollama/chat", { ...payload, stream: true }, signal);

/** Stream a model download. Yields {status, digest?, total?, completed?}. */
export const pullModel = (model, signal) => streamPost("/api/ollama/pull", { model, stream: true }, signal);
