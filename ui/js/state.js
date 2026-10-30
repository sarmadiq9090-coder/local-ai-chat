// state.js — the single shared app state object plus a tiny event emitter.
// Modules mutate `state` directly and then emit() a named event so others can
// re-render. Events used: "settings", "models", "model", "conversations",
// "current", "streaming", "focus-composer", "status".

export const state = {
  settings: null,          // Settings object from the backend (see docs/API.md)
  status: null,            // last /api/app/status response
  models: [],              // installed models from /api/ollama/tags
  caps: {},                // model name -> capabilities[] (cached /api/ollama/show)
  model: null,             // currently selected model name
  conversations: [],       // ConversationMeta[] (newest first)
  current: null,           // full Conversation currently open, or null (new chat)
  streaming: false,        // true while a reply is being generated
  controller: null,        // AbortController of the running generation
  pendingAttachments: [],  // [{name, content}] chips above the composer
  editIndex: null,         // index of the user message being edited, or null
};

const listeners = new Map();

/** Subscribe to an event; returns an unsubscribe function. */
export function on(event, fn) {
  if (!listeners.has(event)) listeners.set(event, new Set());
  listeners.get(event).add(fn);
  return () => listeners.get(event)?.delete(fn);
}

/** Notify all subscribers of `event`. Listener errors are logged, not thrown. */
export function emit(event, payload) {
  for (const fn of listeners.get(event) || []) {
    try { fn(payload); } catch (e) { console.error(`[${event}] listener failed`, e); }
  }
}
