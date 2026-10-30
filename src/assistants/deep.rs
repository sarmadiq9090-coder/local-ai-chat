//! Deep understanding (v2.2): retrieval for one chat turn with query
//! rewriting, context expansion, document digests and a token budget.
//!
//! Per turn ([`AssistantStore::context`]):
//! 1. **Query rewriting** — when the last user message is short or refers to
//!    earlier turns ([`needs_rewrite`]), the chat model rewrites it into a
//!    standalone search query (non-streaming, temperature 0.1, ≤ 60 tokens,
//!    thinking off; few-shot prompt; cached per conversation tail and
//!    [`REWRITE_PROMPT_VERSION`]). The message, the rewrite and the previous
//!    question + message are searched and merged (best score per chunk).
//! 2. **Search** — like `/search` (cosine + keyword boost, `MIN_SCORE`,
//!    `RELATIVE_WINDOW`) over document chunks *and* digest chunks, up to
//!    [`DEEP_K`] hits.
//! 3. **Packing** ([`pack`]) — the knowledge budget ([`knowledge_budget`],
//!    ~70 % of `num_ctx` minus history and answer room) is filled with the
//!    full digests of the best-matching documents first (≤ 35 % of the
//!    budget), then each hit with its neighbours (i−1, i+1) by score, falling
//!    back to the bare hit when the window doesn't fit.
//! 4. **Expansion/merge** — selected chunks of a document that touch are
//!    merged into one passage (overlap removed, [`merge_text`]); passages are
//!    ordered by document (best first), digest first, then by position.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::types::{Assistant, Index};
use super::{AssistantStore, NotFound, ReindexRequired, embed, index, is_stale, llm, same_model};

/// `num_ctx` of a deep-mode assistant unless it sets its own.
pub const DEEP_NUM_CTX: u32 = 16384;
/// Lower/upper bounds accepted for an assistant's `num_ctx`.
pub const MIN_NUM_CTX: u32 = 2048;
pub const MAX_NUM_CTX: u32 = 131_072;
/// Max chunk hits per turn in deep mode (before expansion).
pub const DEEP_K: usize = 10;
/// Share of `num_ctx` the knowledge may fill.
pub const KNOWLEDGE_SHARE: f32 = 0.70;
/// Share of the knowledge budget full digests may take.
pub const DIGEST_SHARE: f32 = 0.35;
/// Full digests of at most this many documents per turn.
pub const MAX_DIGEST_DOCS: usize = 3;
/// Messages longer than this many words without reference words are
/// self-contained (no rewrite).
pub const SELF_CONTAINED_WORDS: usize = 12;

pub const KIND_PASSAGE: &str = "passage";
pub const KIND_DIGEST: &str = "digest";

// ---- token estimate ----------------------------------------------------------

fn is_arabic_char(c: char) -> bool {
    matches!(c, '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
}

/// Rough token count for Qwen/Llama-style tokenizers: Arabic ≈ 2.5 chars per
/// token, other text ≈ 3.5 (measured on qwen3 with the eval documents;
/// deliberately on the high side so the budget isn't overrun).
pub fn estimate_tokens(text: &str) -> usize {
    let (mut ar, mut other) = (0usize, 0usize);
    for c in text.chars() {
        if is_arabic_char(c) {
            ar += 1;
        } else {
            other += 1;
        }
    }
    (ar * 2).div_ceil(5) + (other * 2).div_ceil(7)
}

/// Tokens the knowledge may use: [`KNOWLEDGE_SHARE`] of `num_ctx`, but leave
/// room for the history, the instructions/rules (~500) and the answer (1,500,
/// or 3,000 with thinking). Never below 1/8 of `num_ctx`.
pub fn knowledge_budget(num_ctx: u32, history_tokens: usize, think: bool) -> usize {
    let n = num_ctx as usize;
    let reserve = 500 + if think { 3000 } else { 1500 };
    let share = (n as f32 * KNOWLEDGE_SHARE) as usize;
    share
        .min(n.saturating_sub(history_tokens + reserve))
        .max(n / 8)
}

// ---- query rewriting -----------------------------------------------------------

/// Words that point back at an earlier turn (EN + AR, folded: no hamza/diacritics).
const REFERENCE_WORDS: &[&str] = &[
    "it",
    "its",
    "this",
    "that",
    "these",
    "those",
    "they",
    "them",
    "their",
    "he",
    "she",
    "him",
    "his",
    "her",
    "there",
    "same",
    "above",
    "previous",
    "former",
    "latter",
    "also",
    "too",
    "else",
    "other",
    "others",
    "one",
    "ones",
    "more",
    "again",
    "هو",
    "هي",
    "هم",
    "هما",
    "هن",
    "هذا",
    "هذه",
    "ذلك",
    "تلك",
    "هؤلاء",
    "اولئك",
    "هناك",
    "نفس",
    "ايضا",
    "كذلك",
    "السابق",
    "السابقه",
    "المذكور",
    "المذكوره",
    "عنه",
    "عنها",
    "عنهم",
    "منه",
    "منها",
    "فيه",
    "فيها",
    "له",
    "لها",
    "لهم",
    "به",
    "بها",
    "ذاك",
    "الاخر",
    "الاخري",
    "الباقي",
];

/// Openers of a follow-up ("and …", "what about …", «وماذا عن …»).
const FOLLOW_UP_OPENERS: &[&str] = &[
    "and ",
    "or ",
    "but ",
    "so ",
    "then ",
    "what about",
    "how about",
    "what if",
    "same for",
    "وماذا",
    "ماذا عن",
    "وكم",
    "وهل",
    "وما",
    "ومتى",
    "واين",
    "وأين",
    "وكيف",
    "ولماذا",
    "طيب",
    "ثم ",
    "لكن ",
    "او ",
    "أو ",
];

fn fold_word(w: &str) -> String {
    w.chars()
        .filter(|c| !matches!(c, '\u{064B}'..='\u{0652}' | '\u{0640}'))
        .map(|c| match c {
            'أ' | 'إ' | 'آ' => 'ا',
            'ى' => 'ي',
            'ة' => 'ه',
            _ => c.to_lowercase().next().unwrap_or(c),
        })
        .collect()
}

/// Whether the last user message should be rewritten into a standalone query:
/// only when there is an earlier turn, and the message is short
/// (≤ [`SELF_CONTAINED_WORDS`] words), opens like a follow-up, or contains a
/// reference word (pronoun, "same", «نفس»…).
pub fn needs_rewrite(last: &str, has_history: bool) -> bool {
    let last = last.trim();
    if !has_history || last.is_empty() {
        return false;
    }
    let lower = last.to_lowercase();
    if FOLLOW_UP_OPENERS.iter().any(|o| lower.starts_with(o)) {
        return true;
    }
    let words: Vec<String> = last
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '\u{064B}'..='\u{0652}')))
        .filter(|w| !w.is_empty())
        .map(fold_word)
        .collect();
    if words.len() <= SELF_CONTAINED_WORDS {
        return true;
    }
    words.iter().any(|w| REFERENCE_WORDS.contains(&w.as_str()))
}

/// A chat turn sent by the UI (only role + content are used).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

/// Index of the last non-empty user message.
fn last_user(messages: &[ChatTurn]) -> Option<usize> {
    messages
        .iter()
        .rposition(|m| m.role == "user" && !m.content.trim().is_empty())
}

fn truncate_chars(s: &str, n: usize) -> String {
    let s = s.trim();
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// Version of the rewrite prompt; part of the rewrite cache key so a changed
/// prompt never reuses rewrites made by an older one.
pub const REWRITE_PROMPT_VERSION: u32 = 2;

/// System prompt of the rewrite call: rules plus few-shot examples (Arabic
/// and English) of replacing only the slot the user changed.
const REWRITE_SYSTEM: &str = "You rewrite the user's LAST message into a standalone search \
query for a document search engine. Use the earlier conversation only to resolve what the last \
message refers to (pronouns, \"and X?\", \"what about…\", «وماذا عن…»).
Rules:
- Write ONE complete, standalone question in the SAME LANGUAGE as the last message.
- Start from the previous question and replace ONLY the part the user changed; keep every other \
part (places, people, products, numbers, the subject) exactly as it was. Never swap or drop the \
parts the user did not mention.
- Do not start with \"and\", \"so\", «و» or «طيب».
- Output only the query on one line: no answer, no explanation, no quotes.

Examples:
Conversation:
User: كم سعر الشحن من الرياض إلى جدة؟
Last message: وماذا عن الدمام؟
Standalone search query: كم سعر الشحن من الرياض إلى الدمام؟

Conversation:
User: ما شروط الإجازة السنوية للموظف الجديد؟
Assistant: يستحق الموظف الجديد الإجازة بعد إكمال ستة أشهر…
Last message: وكم مدتها؟
Standalone search query: كم مدة الإجازة السنوية للموظف الجديد؟

Conversation:
User: What is the warranty period of the X200 vacuum?
Assistant: The X200 has a two-year warranty.
Last message: and the X300?
Standalone search query: What is the warranty period of the X300 vacuum?

Conversation:
User: Which documents do I need to open a personal account?
Last message: is it the same for companies?
Standalone search query: Which documents does a company need to open an account?";

/// System + user prompt for the rewrite call: the last two earlier turns
/// (replies cut to 300 chars) and the message to rewrite, in the same
/// layout as the examples.
pub fn rewrite_prompt(messages: &[ChatTurn], last: usize) -> (String, String) {
    let system = REWRITE_SYSTEM.to_string();
    let mut ctx = Vec::new();
    let mut users = 0;
    for m in messages[..last].iter().rev() {
        if m.content.trim().is_empty() {
            continue;
        }
        let (label, text) = if m.role == "user" {
            users += 1;
            ("User", truncate_chars(&m.content, 400))
        } else {
            ("Assistant", truncate_chars(&m.content, 300))
        };
        ctx.push(format!("{label}: {text}"));
        if users >= 2 {
            break;
        }
    }
    ctx.reverse();
    let user = format!(
        "Conversation:\n{}\nLast message: {}\nStandalone search query:",
        ctx.join("\n"),
        messages[last].content.trim()
    );
    (system, user)
}

/// Rewrite cache key: assistant, model, prompt version and the prompt itself.
pub fn rewrite_cache_key(assistant_id: &str, model: &str, user_prompt: &str) -> String {
    format!("{assistant_id}\u{0}{model}\u{0}v{REWRITE_PROMPT_VERSION}\u{0}{user_prompt}")
}

/// Words a rewrite may open with after a stray «و» ("and"): «وماذا» → «ماذا».
const AR_AFTER_WAW: &[&str] = &[
    "ماذا",
    "ما",
    "كم",
    "هل",
    "متى",
    "أين",
    "اين",
    "كيف",
    "لماذا",
    "من",
    "في",
    "إلى",
    "الى",
    "عن",
    "لكن",
    "أيضا",
    "أيضًا",
    "بالنسبة",
];

/// Drop a leading conjunction ("and", "so", «و», «طيب») from a rewrite.
fn strip_leading_conjunction(q: &str) -> &str {
    let mut q = q.trim();
    loop {
        let before = q;
        for p in [
            "and ",
            "And ",
            "AND ",
            "so ",
            "So ",
            "also ",
            "Also ",
            "طيب، ",
            "طيب ",
            "و ",
        ] {
            if let Some(rest) = q.strip_prefix(p) {
                q = rest.trim_start();
            }
        }
        if let Some(rest) = q.strip_prefix('و') {
            let word: String = rest.chars().take_while(|c| c.is_alphabetic()).collect();
            if AR_AFTER_WAW.contains(&word.as_str()) {
                q = rest;
            }
        }
        if q == before {
            return q;
        }
    }
}

/// First meaningful line of the model's reply, without labels/quotes or a
/// leading conjunction; `None` if empty or implausibly long.
pub fn clean_rewrite(reply: &str) -> Option<String> {
    let line = reply
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())?
        .trim_start_matches(|c: char| c == '-' || c == '*' || c.is_whitespace());
    let line = [
        "Standalone search query:",
        "Search query:",
        "Query:",
        "الاستعلام:",
    ]
    .iter()
    .fold(line, |l, p| l.strip_prefix(p).unwrap_or(l))
    .trim()
    .trim_matches(|c| matches!(c, '"' | '\'' | '«' | '»' | '“' | '”' | '`'))
    .trim();
    let line = strip_leading_conjunction(line);
    (!line.is_empty() && line.chars().count() <= 300).then(|| line.to_string())
}

/// Cheap fallback query for a follow-up when no rewrite is available: the
/// previous user message + the last one.
fn heuristic_query(messages: &[ChatTurn], last: usize) -> Option<String> {
    let prev = messages[..last]
        .iter()
        .rev()
        .find(|m| m.role == "user" && !m.content.trim().is_empty())?;
    Some(format!(
        "{}\n{}",
        prev.content.trim(),
        messages[last].content.trim()
    ))
}

// ---- passages / packing ----------------------------------------------------------

/// One knowledge passage given to the model.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Passage {
    pub doc_id: String,
    pub doc_name: String,
    /// [`KIND_PASSAGE`] or [`KIND_DIGEST`].
    pub kind: &'static str,
    pub text: String,
    /// Best hit score inside the passage (a full digest: its document's best score).
    pub score: f32,
    /// Chunk positions merged into this passage (empty for a full digest).
    pub chunks: Vec<usize>,
    /// Positions that were search hits (the rest are neighbours).
    pub hits: Vec<usize>,
    /// Contains neighbour chunks added by context expansion.
    pub expanded: bool,
    /// Whole digest of the document (not just a matching piece).
    pub full_digest: bool,
    pub tokens: usize,
}

/// Append `b` to `a` for adjacent chunks: drop up to two leading lines of `b`
/// already present in `a` (a repeated section heading, a repeated overlap
/// line), then the longest suffix of `a` that starts `b` (≥ 12 chars; the
/// overlap of a cut line).
pub fn merge_text(a: &str, b: &str) -> String {
    let mut rest = b;
    for _ in 0..2 {
        let (line, after) = match rest.split_once('\n') {
            Some((l, r)) => (l, r),
            None => break,
        };
        let l = line.trim();
        if l.chars().count() >= 3 && a.contains(l) {
            rest = after;
        } else {
            break;
        }
    }
    let rest = rest.trim_start_matches('\n');
    // Longest suffix of `a` (in chars, ≤ 400) that is a prefix of `rest`.
    let a_chars: Vec<(usize, char)> = a.char_indices().collect();
    let max = a_chars.len().min(400);
    for take in (12..=max).rev() {
        let start = a_chars[a_chars.len() - take].0;
        let suffix = &a[start..];
        if let Some(r) = rest.strip_prefix(suffix) {
            return format!("{a}{r}");
        }
    }
    if rest.is_empty() {
        return a.to_string();
    }
    format!("{a}\n{rest}")
}

/// What [`pack`] works on: the text chunks of each document by position, the
/// digest chunks, full digest texts and the search hits.
pub struct PackInput<'a> {
    /// `(doc_id, kind_is_digest, position) → text` for every chunk.
    pub chunks: HashMap<(&'a str, bool, usize), &'a str>,
    /// Hits: `(doc_id, is_digest, position, score)`, any order.
    pub hits: Vec<(&'a str, bool, usize, f32)>,
    /// Full digest text per document (only documents that have one).
    pub digests: HashMap<&'a str, &'a str>,
    /// Display name per document; hits of unknown documents are ignored.
    pub names: HashMap<&'a str, &'a str>,
    pub budget: usize,
}

/// Fill the budget (see the module docs) and return the passages in output
/// order, plus the tokens used.
pub fn pack(input: &PackInput<'_>) -> (Vec<Passage>, usize) {
    let mut hits: Vec<_> = input
        .hits
        .iter()
        .filter(|h| input.names.contains_key(h.0))
        .copied()
        .collect();
    hits.sort_by(|a, b| b.3.total_cmp(&a.3));
    let mut doc_best: Vec<(&str, f32)> = Vec::new();
    for h in &hits {
        if !doc_best.iter().any(|d| d.0 == h.0) {
            doc_best.push((h.0, h.3));
        }
    }
    let mut used = 0usize;
    let mut out_digests: HashMap<&str, Passage> = HashMap::new();

    // 1. Full digests of the best documents.
    let digest_budget = (input.budget as f32 * DIGEST_SHARE) as usize;
    let mut digest_used = 0usize;
    for &(doc, score) in doc_best.iter().take(MAX_DIGEST_DOCS) {
        let Some(text) = input.digests.get(doc) else {
            continue;
        };
        let tokens = estimate_tokens(text);
        if digest_used + tokens <= digest_budget && used + tokens <= input.budget {
            digest_used += tokens;
            used += tokens;
            out_digests.insert(
                doc,
                Passage {
                    doc_id: doc.to_string(),
                    doc_name: input.names[doc].to_string(),
                    kind: KIND_DIGEST,
                    text: text.trim().to_string(),
                    score,
                    chunks: Vec::new(),
                    hits: Vec::new(),
                    expanded: false,
                    full_digest: true,
                    tokens,
                },
            );
        }
    }

    // 2. Hits with their neighbours, best first.
    // Selected positions per (doc, is_digest): position → is_hit.
    let mut selected: BTreeMap<(&str, bool), BTreeMap<usize, Option<f32>>> = BTreeMap::new();
    let tok = |doc: &str, dg: bool, i: usize| {
        input
            .chunks
            .get(&(doc, dg, i))
            .map_or(0, |t| estimate_tokens(t))
    };
    for &(doc, dg, i, score) in &hits {
        if dg && out_digests.contains_key(doc) {
            continue; // the whole digest is already in
        }
        let sel = selected.entry((doc, dg)).or_default();
        if let Some(Some(_)) = sel.get(&i) {
            continue;
        }
        let mut window = vec![i];
        if !dg {
            if i > 0 && input.chunks.contains_key(&(doc, false, i - 1)) {
                window.push(i - 1);
            }
            if input.chunks.contains_key(&(doc, false, i + 1)) {
                window.push(i + 1);
            }
        }
        let new: Vec<usize> = window
            .iter()
            .copied()
            .filter(|p| !sel.contains_key(p))
            .collect();
        let cost: usize = new.iter().map(|&p| tok(doc, dg, p)).sum();
        if used + cost <= input.budget {
            used += cost;
            for p in new {
                sel.insert(p, None);
            }
            sel.insert(i, Some(score));
        } else if !sel.contains_key(&i) && used + tok(doc, dg, i) <= input.budget {
            used += tok(doc, dg, i);
            sel.insert(i, Some(score));
        } else if sel.contains_key(&i) {
            sel.insert(i, Some(score)); // a neighbour that was a hit too
        }
    }

    // 3. Merge runs of touching positions into passages.
    let mut runs: Vec<Passage> = Vec::new();
    for ((doc, dg), positions) in &selected {
        let mut run: Vec<(usize, Option<f32>)> = Vec::new();
        let flush = |run: &mut Vec<(usize, Option<f32>)>, runs: &mut Vec<Passage>| {
            if run.is_empty() {
                return;
            }
            let mut text = String::new();
            for (p, _) in run.iter() {
                let t = input.chunks.get(&(*doc, *dg, *p)).copied().unwrap_or("");
                text = if text.is_empty() {
                    t.to_string()
                } else {
                    merge_text(&text, t)
                };
            }
            let hits: Vec<usize> = run.iter().filter(|r| r.1.is_some()).map(|r| r.0).collect();
            let score = run.iter().filter_map(|r| r.1).fold(f32::MIN, f32::max);
            if hits.is_empty() {
                run.clear();
                return;
            }
            let tokens = estimate_tokens(&text);
            runs.push(Passage {
                doc_id: doc.to_string(),
                doc_name: input.names[doc].to_string(),
                kind: if *dg { KIND_DIGEST } else { KIND_PASSAGE },
                text,
                score,
                chunks: run.iter().map(|r| r.0).collect(),
                expanded: hits.len() < run.len(),
                hits,
                full_digest: false,
                tokens,
            });
            run.clear();
        };
        for (&p, &s) in positions {
            if run.last().is_some_and(|l| l.0 + 1 != p) {
                flush(&mut run, &mut runs);
            }
            run.push((p, s));
        }
        flush(&mut run, &mut runs);
    }

    // 4. Order: documents by best score; digest first, then by position.
    let rank = |doc: &str| {
        doc_best
            .iter()
            .position(|d| d.0 == doc)
            .unwrap_or(usize::MAX)
    };
    let mut out: Vec<Passage> = out_digests.into_values().collect();
    out.extend(runs);
    out.sort_by(|a, b| {
        rank(&a.doc_id)
            .cmp(&rank(&b.doc_id))
            .then((a.kind != KIND_DIGEST).cmp(&(b.kind != KIND_DIGEST)))
            .then(b.full_digest.cmp(&a.full_digest))
            .then(a.chunks.first().cmp(&b.chunks.first()))
    });
    (out, used)
}

// ---- the per-turn context --------------------------------------------------------

/// Body of `POST /assistants/{id}/context`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ContextRequest {
    /// The conversation so far, ending with the user's question.
    pub messages: Vec<ChatTurn>,
    /// Chat model that will answer (the UI's choice); used for rewriting.
    pub model: Option<String>,
    /// Max hits (default [`DEEP_K`], max 50).
    pub k: Option<usize>,
    /// `false` disables query rewriting for this call.
    pub rewrite: Option<bool>,
}

/// App-wide defaults the context depends on.
#[derive(Clone, Debug, Default)]
pub struct AppDefaults {
    pub num_ctx: u32,
    pub think: bool,
    pub default_model: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ContextTimings {
    pub rewrite_ms: u64,
    pub search_ms: u64,
    pub total_ms: u64,
}

/// Response of `POST /assistants/{id}/context`.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ContextResult {
    /// The last user message.
    pub query: String,
    /// Standalone query written by the model (`None` = not rewritten).
    pub rewritten: Option<String>,
    /// `"skipped"` (self-contained / no history / off), `"done"`, `"cached"`,
    /// `"failed"` (the heuristic follow-up query was used instead).
    pub rewrite: &'static str,
    /// Queries actually searched.
    pub queries: Vec<String>,
    pub deep: bool,
    /// Effective `options.num_ctx` for the answer.
    pub num_ctx: u32,
    /// Effective thinking switch for the answer (if the model supports it).
    pub think: bool,
    pub budget_tokens: usize,
    pub used_tokens: usize,
    pub passages: Vec<Passage>,
    pub timings: ContextTimings,
}

/// Effective `(num_ctx, think)` of an assistant.
pub fn effective_options(a: &Assistant, app: &AppDefaults) -> (u32, bool) {
    let default_ctx = if a.deep { DEEP_NUM_CTX } else { app.num_ctx };
    let num_ctx = a
        .num_ctx
        .unwrap_or(default_ctx)
        .clamp(MIN_NUM_CTX, MAX_NUM_CTX);
    let think = a.think.unwrap_or(if a.deep { true } else { app.think });
    (num_ctx, think)
}

/// Combined score per chunk for several queries: max over queries of
/// cosine + keyword boost.
fn score_multi(idx: &Index, qvecs: &[Vec<f32>], queries: &[String]) -> Vec<(usize, f32)> {
    let kws: Vec<HashSet<String>> = queries.iter().map(|q| index::keywords(q)).collect();
    idx.chunks
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let best = qvecs
                .iter()
                .zip(&kws)
                .filter(|(q, _)| q.len() == c.vec.len())
                .map(|(q, kw)| index::dot(&c.vec, q) + index::keyword_boost(kw, &c.text))
                .fold(f32::MIN, f32::max);
            (best > f32::MIN).then_some((i, best))
        })
        .collect()
}

impl AssistantStore {
    fn rewrite_cache_get(&self, key: &str) -> Option<String> {
        let c = self
            .inner
            .rewrites
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        c.get(key).cloned()
    }

    fn rewrite_cache_put(&self, key: String, value: String) {
        let mut c = self
            .inner
            .rewrites
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if c.len() >= 256 {
            c.clear();
        }
        c.insert(key, value);
    }

    /// Retrieval for one chat turn (see the module docs). Works for any
    /// assistant; rewriting/expansion/digests/budget apply in deep mode only
    /// (otherwise it is `/search` with k = 5 plus the heuristic follow-up
    /// query). `Err(ReindexRequired)` on a stale index.
    pub async fn context(
        &self,
        ollama_url: &str,
        id: &str,
        req: ContextRequest,
        app: &AppDefaults,
    ) -> anyhow::Result<ContextResult> {
        let started = Instant::now();
        let a = self.get(id)?.ok_or(NotFound("assistant"))?;
        let (num_ctx, think) = effective_options(&a, app);
        let mut res = ContextResult {
            deep: a.deep,
            num_ctx,
            think,
            rewrite: "skipped",
            ..Default::default()
        };
        let Some(li) = last_user(&req.messages) else {
            return Ok(res);
        };
        let last = req.messages[li].content.trim().to_string();
        res.query = last.clone();
        let idx = self.load_index(id).await?;
        if idx.chunks.is_empty() {
            return Ok(res);
        }
        if !same_model(&idx.embed_model, &a.embed_model) || is_stale(&a, &idx) {
            return Err(ReindexRequired(
                "the knowledge index is out of date; re-index required".into(),
            )
            .into());
        }

        // 1. Queries.
        let has_history = req.messages[..li]
            .iter()
            .any(|m| m.role == "user" && !m.content.trim().is_empty());
        let mut queries = vec![last.clone()];
        let model = req
            .model
            .clone()
            .or_else(|| a.model.clone())
            .or_else(|| app.default_model.clone())
            .filter(|m| !m.trim().is_empty());
        let t0 = Instant::now();
        if a.deep && req.rewrite != Some(false) && needs_rewrite(&last, has_history) {
            let (system, user) = rewrite_prompt(&req.messages, li);
            let key = rewrite_cache_key(&a.id, model.as_deref().unwrap_or(""), &user);
            if let Some(q) = self.rewrite_cache_get(&key) {
                res.rewritten = Some(q);
                res.rewrite = "cached";
            } else if let Some(m) = &model {
                let call = llm::ChatRequest {
                    model: m,
                    system: &system,
                    user: &user,
                    // Same num_ctx as the answer: Ollama reloads a model whose
                    // context size changes.
                    num_ctx,
                    num_predict: 60,
                    temperature: 0.1,
                    timeout: Duration::from_secs(60),
                };
                match llm::chat(&self.inner.http, ollama_url, &call, &|| false).await {
                    Ok(reply) => match clean_rewrite(&reply) {
                        Some(q) => {
                            self.rewrite_cache_put(key, q.clone());
                            res.rewritten = Some(q);
                            res.rewrite = "done";
                        }
                        None => res.rewrite = "failed",
                    },
                    Err(e) => {
                        eprintln!("assistants: query rewrite failed: {e:#}");
                        res.rewrite = "failed";
                    }
                }
            } else {
                res.rewrite = "failed";
            }
        }
        res.timings.rewrite_ms = t0.elapsed().as_millis() as u64;
        match &res.rewritten {
            Some(q) if q.trim() != last => {
                queries.push(q.clone());
                // A rewrite can still pick the wrong slot («من البصرة» instead
                // of «إلى البصرة»): also search the previous question + the
                // message, so every entity of both is searched.
                if let Some(h) = heuristic_query(&req.messages, li) {
                    queries.push(h);
                }
            }
            Some(_) => {}
            None if has_history && (res.rewrite == "failed" || !a.deep) => {
                // Follow-up without a rewrite: also search with the previous question.
                if needs_rewrite(&last, true)
                    && last.chars().count() < 60
                    && let Some(q) = heuristic_query(&req.messages, li)
                {
                    queries.push(q);
                }
            }
            None => {}
        }

        // 2. Search.
        let t1 = Instant::now();
        let qvecs = embed::embed(&self.inner.http, ollama_url, &idx.embed_model, &queries).await?;
        let mut scored = score_multi(&idx, &qvecs, &queries);
        if !a.deep {
            scored.retain(|&(i, _)| idx.chunks[i].kind.is_none());
        }
        let k = req
            .k
            .unwrap_or(if a.deep { DEEP_K } else { index::DEFAULT_K })
            .clamp(1, 50);
        let top = index::top_k_from(scored, k, index::MIN_SCORE);
        res.timings.search_ms = t1.elapsed().as_millis() as u64;
        res.queries = queries;

        let names: HashMap<&str, &str> = a
            .docs
            .iter()
            .map(|d| (d.id.as_str(), d.name.as_str()))
            .collect();
        if !a.deep {
            res.passages = top
                .iter()
                .filter_map(|&(i, score)| {
                    let c = &idx.chunks[i];
                    let name = names.get(c.doc_id.as_str())?;
                    Some(Passage {
                        doc_id: c.doc_id.clone(),
                        doc_name: name.to_string(),
                        kind: KIND_PASSAGE,
                        text: c.text.clone(),
                        score,
                        chunks: vec![c.i],
                        hits: vec![c.i],
                        expanded: false,
                        full_digest: false,
                        tokens: estimate_tokens(&c.text),
                    })
                })
                .collect();
            res.used_tokens = res.passages.iter().map(|p| p.tokens).sum();
            res.timings.total_ms = started.elapsed().as_millis() as u64;
            return Ok(res);
        }

        // 3. Pack.
        let history_tokens: usize = req.messages[..=li]
            .iter()
            .map(|m| estimate_tokens(&m.content))
            .sum();
        let budget = knowledge_budget(num_ctx, history_tokens, think);
        let mut digests_owned: Vec<(String, String)> = Vec::new();
        let hit_docs: HashSet<&str> = top
            .iter()
            .map(|&(i, _)| idx.chunks[i].doc_id.as_str())
            .collect();
        for d in &a.docs {
            if hit_docs.contains(d.id.as_str())
                && d.digest.as_ref().is_some_and(|g| g.status != "failed")
                && let Ok(Some(text)) = self.digest_text(&a.id, &d.id)
            {
                digests_owned.push((d.id.clone(), text));
            }
        }
        let input = PackInput {
            chunks: idx
                .chunks
                .iter()
                .map(|c| ((c.doc_id.as_str(), c.kind.is_some(), c.i), c.text.as_str()))
                .collect(),
            hits: top
                .iter()
                .map(|&(i, s)| {
                    let c = &idx.chunks[i];
                    (c.doc_id.as_str(), c.kind.is_some(), c.i, s)
                })
                .collect(),
            digests: digests_owned
                .iter()
                .map(|(d, t)| (d.as_str(), t.as_str()))
                .collect(),
            names,
            budget,
        };
        let (passages, used) = pack(&input);
        res.budget_tokens = budget;
        res.used_tokens = used;
        res.passages = passages;
        res.timings.total_ms = started.elapsed().as_millis() as u64;
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_skip_heuristic() {
        // No history: never.
        assert!(!needs_rewrite("and for managers?", false));
        // Short or follow-up: yes.
        assert!(needs_rewrite("and for managers?", true));
        assert!(needs_rewrite("وماذا عن المدراء؟", true));
        assert!(needs_rewrite("كم مدتها؟", true));
        assert!(needs_rewrite("How long is it?", true));
        // Long and self-contained: no.
        assert!(!needs_rewrite(
            "How many days of annual leave does a full-time employee get after five years of service in the company?",
            true
        ));
        assert!(!needs_rewrite(
            "كم يبلغ بدل السكن الشهري للموظف المتزوج في الدرجة الثالثة حسب لائحة البدلات المعتمدة في الشركة؟",
            true
        ));
        // Long but with a reference word: yes.
        assert!(needs_rewrite(
            "Can you explain in more detail how that policy applies to employees who joined during the probation period?",
            true
        ));
        assert!(needs_rewrite(
            "هل يمكنك أن تشرح بتفصيل أكثر كيف تنطبق هذه السياسة على الموظفين الذين انضموا خلال فترة التجربة؟",
            true
        ));
    }

    #[test]
    fn rewrite_reply_is_cleaned() {
        assert_eq!(
            clean_rewrite("Query: \"annual leave for managers\"\n(explanation)"),
            Some("annual leave for managers".into())
        );
        assert_eq!(
            clean_rewrite("\n«بدل السكن للمتزوج»"),
            Some("بدل السكن للمتزوج".into())
        );
        assert_eq!(clean_rewrite("   "), None);
        assert_eq!(clean_rewrite(&"x".repeat(400)), None);
        // Leading conjunctions are dropped; words that merely start with و stay.
        assert_eq!(
            clean_rewrite("وماذا عن مدة التوصيل من بغداد إلى البصرة؟"),
            Some("ماذا عن مدة التوصيل من بغداد إلى البصرة؟".into())
        );
        assert_eq!(
            clean_rewrite("Standalone search query: and what is the X300 warranty?"),
            Some("what is the X300 warranty?".into())
        );
        assert_eq!(clean_rewrite("و كم مدتها؟"), Some("كم مدتها؟".into()));
        assert_eq!(
            clean_rewrite("وثيقة التأمين الصحي"),
            Some("وثيقة التأمين الصحي".into())
        );
        assert_eq!(
            clean_rewrite("Andorra visa rules"),
            Some("Andorra visa rules".into())
        );
    }

    #[test]
    fn rewrite_prompt_has_slot_examples_and_versioned_cache_key() {
        let (system, _) = rewrite_prompt(
            &[
                ChatTurn {
                    role: "user".into(),
                    content: "ما مدة التوصيل من بغداد إلى أربيل؟".into(),
                },
                ChatTurn {
                    role: "user".into(),
                    content: "وماذا عن البصرة؟".into(),
                },
            ],
            1,
        );
        // Few-shot: an Arabic slot replacement and English follow-ups.
        assert!(system.contains("Last message: وماذا عن الدمام؟"));
        assert!(system.contains("كم سعر الشحن من الرياض إلى الدمام؟"));
        assert!(system.contains("What is the warranty period of the X300 vacuum?"));
        assert!(system.contains("replace ONLY the part the user changed"));
        let key = rewrite_cache_key("a1", "qwen3:8b", "prompt");
        assert!(key.contains(&format!("v{REWRITE_PROMPT_VERSION}")));
        assert_ne!(
            key, "a1\u{0}qwen3:8b\u{0}prompt",
            "old (v1) keys never match"
        );
    }

    #[test]
    fn rewrite_prompt_uses_last_turns() {
        let m = |r: &str, c: &str| ChatTurn {
            role: r.into(),
            content: c.into(),
        };
        let msgs = vec![
            m("user", "old question"),
            m("assistant", "old answer"),
            m("user", "What is the housing allowance?"),
            m("assistant", &"long answer ".repeat(100)),
            m("user", "and for managers?"),
        ];
        let (_, user) = rewrite_prompt(&msgs, 4);
        assert!(user.contains("User: old question"));
        assert!(user.contains("User: What is the housing allowance?"));
        assert!(user.contains("Last message: and for managers?"));
        assert!(user.chars().count() < 1200, "assistant replies are cut");
        assert_eq!(
            heuristic_query(&msgs, 4).unwrap(),
            "What is the housing allowance?\nand for managers?"
        );
    }

    #[test]
    fn token_estimate_and_budget() {
        assert_eq!(estimate_tokens(""), 0);
        let en = estimate_tokens(&"word ".repeat(100)); // 500 chars
        assert!((130..=160).contains(&en), "{en}");
        let ar = estimate_tokens(&"كلمة ".repeat(100)); // 400 Arabic + 100 spaces
        assert!(ar > en / 2 && ar < 220, "{ar}");
        // 70 % of 16k when the history is small.
        assert_eq!(knowledge_budget(16384, 100, false), 11468);
        // A long history shrinks it, never below 1/8.
        assert_eq!(knowledge_budget(16384, 10_000, true), 16384 - 13_500);
        assert_eq!(knowledge_budget(8192, 50_000, true), 1024);
    }

    #[test]
    fn effective_options_follow_deep_mode() {
        let app = AppDefaults {
            num_ctx: 8192,
            think: false,
            default_model: None,
        };
        let mut a = Assistant::default();
        assert_eq!(effective_options(&a, &app), (8192, false));
        a.deep = true;
        assert_eq!(effective_options(&a, &app), (DEEP_NUM_CTX, true));
        a.num_ctx = Some(12288);
        a.think = Some(false);
        assert_eq!(effective_options(&a, &app), (12288, false));
        a.num_ctx = Some(10);
        assert_eq!(effective_options(&a, &app).0, MIN_NUM_CTX);
    }

    #[test]
    fn merge_removes_heading_and_overlap() {
        // Continuation chunk repeating the section heading.
        let a = "## Leave\nAnnual leave is 21 days.";
        let b = "## Leave\nSick leave is 10 days.";
        assert_eq!(
            merge_text(a, b),
            "## Leave\nAnnual leave is 21 days.\nSick leave is 10 days."
        );
        // A cut line repeated as overlap.
        let a = "The allowance is paid monthly to every employee in grade three";
        let b = "to every employee in grade three and above.";
        assert_eq!(
            merge_text(a, b),
            "The allowance is paid monthly to every employee in grade three and above."
        );
        // Unrelated chunks are joined by a newline.
        assert_eq!(merge_text("One.", "Two."), "One.\nTwo.");
    }

    fn input<'a>(
        chunks: &'a [(&'a str, bool, usize, String)],
        hits: Vec<(&'a str, bool, usize, f32)>,
        digests: HashMap<&'a str, &'a str>,
        budget: usize,
    ) -> PackInput<'a> {
        PackInput {
            chunks: chunks
                .iter()
                .map(|(d, g, i, t)| ((*d, *g, *i), t.as_str()))
                .collect(),
            hits,
            digests,
            names: HashMap::from([("A", "a.pdf"), ("B", "b.pdf")]),
            budget,
        }
    }

    fn doc_chunks() -> Vec<(&'static str, bool, usize, String)> {
        let mut v = Vec::new();
        for i in 0..6 {
            v.push(("A", false, i, format!("A{i} {}", "x".repeat(70))));
        }
        for i in 0..3 {
            v.push(("B", false, i, format!("B{i} {}", "y".repeat(70))));
        }
        v.push(("A", true, 0, "digest piece A0".to_string()));
        v
    }

    #[test]
    fn context_expansion_merges_neighbours_in_order() {
        let chunks = doc_chunks();
        // Hits A3 (best), A1, B2.
        let inp = input(
            &chunks,
            vec![
                ("B", false, 2, 0.5),
                ("A", false, 3, 0.8),
                ("A", false, 1, 0.6),
            ],
            HashMap::new(),
            10_000,
        );
        let (ps, used) = pack(&inp);
        // A1 and A3 windows (0..=4) touch → one passage; B1..=2 another.
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].doc_id, "A");
        assert_eq!(ps[0].chunks, vec![0, 1, 2, 3, 4]);
        assert_eq!(ps[0].hits, vec![1, 3]);
        assert!(ps[0].expanded);
        assert!((ps[0].score - 0.8).abs() < 1e-6);
        let t = &ps[0].text;
        assert!(t.find("A0").unwrap() < t.find("A1").unwrap());
        assert!(t.find("A3").unwrap() < t.find("A4").unwrap());
        assert_eq!(ps[1].doc_id, "B");
        assert_eq!(ps[1].chunks, vec![1, 2]); // no B3
        // `used` counts chunk costs; merged passages differ only by joins.
        let merged: usize = ps.iter().map(|p| p.tokens).sum();
        assert!(used.abs_diff(merged) <= 4, "{used} vs {merged}");
    }

    #[test]
    fn budget_packing_prefers_digest_then_falls_back_to_bare_hits() {
        let chunks = doc_chunks();
        let one = estimate_tokens(&chunks[0].3);
        let digest = "Overview of A. ".repeat(4);
        let dtok = estimate_tokens(&digest);
        let digests = HashMap::from([("A", digest.as_str())]);
        // Budget: digest + 3 chunks + a bit — the best hit gets its full
        // window (3), the second only fits nothing more.
        let budget = ((dtok as f32 / DIGEST_SHARE) as usize).max(dtok + 4 * one);
        let inp = input(
            &chunks,
            vec![
                ("A", false, 3, 0.8),
                ("A", true, 0, 0.7), // digest piece: skipped, whole digest is in
                ("B", false, 1, 0.6),
            ],
            digests.clone(),
            budget,
        );
        let (ps, used) = pack(&inp);
        assert!(used <= budget);
        assert_eq!(ps[0].kind, KIND_DIGEST);
        assert!(ps[0].full_digest);
        assert_eq!(ps[1].chunks, vec![2, 3, 4]);
        // B1's window (3 chunks) didn't fit; the bare hit did.
        let b = ps.iter().find(|p| p.doc_id == "B").unwrap();
        assert_eq!(b.chunks, vec![1]);
        assert!(!b.expanded);
        assert!(!ps.iter().any(|p| p.kind == KIND_DIGEST && !p.full_digest));

        // Tiny budget: the digest doesn't fit its share; hits still come in.
        let inp = input(
            &chunks,
            vec![("A", false, 3, 0.8), ("A", true, 0, 0.7)],
            digests,
            one + 2,
        );
        let (ps, used) = pack(&inp);
        assert!(used <= one + 2);
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].chunks, vec![3]);
        // Unknown documents and an empty hit list give nothing.
        let inp = input(&chunks, vec![("Z", false, 0, 0.9)], HashMap::new(), 1000);
        assert!(pack(&inp).0.is_empty());
    }

    #[test]
    fn digest_pieces_are_used_when_the_full_digest_is_missing() {
        let chunks = doc_chunks();
        let inp = input(&chunks, vec![("A", true, 0, 0.7)], HashMap::new(), 1000);
        let (ps, _) = pack(&inp);
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].kind, KIND_DIGEST);
        assert!(!ps[0].full_digest);
        assert_eq!(ps[0].text, "digest piece A0");
    }
}
