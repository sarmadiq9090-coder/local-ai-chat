//! Document digests (deep understanding): the assistant's chat model reads a
//! document and writes a structured summary in the document's language —
//! overview, outline, key facts/numbers/dates/names, conditions/exceptions,
//! definitions. Stored as `docs/{doc_id}.digest.md` and embedded as extra
//! chunks (`kind: "digest"`) so overview questions find it.
//!
//! Long documents are map-reduced: the text is cut into ~[`WINDOW_CHARS`]
//! windows on paragraph boundaries, each window gets a digest, then the
//! partial digests are merged (in groups of ≤ [`REDUCE_INPUT_CHARS`],
//! repeatedly) into one. Digests are optional: a missing model, a timeout or
//! a cancel leaves the document indexed without one (`DigestMeta.status =
//! "failed"` with the reason).

use std::time::{Duration, Instant};

use anyhow::bail;

use super::types::{Assistant, DigestMeta, Index, IndexedChunk, Progress};
use super::{AssistantStore, NotFound, chunk, deep, index, llm, now_ms};
use crate::config::write_atomic;

/// Characters of document text per map step.
pub const WINDOW_CHARS: usize = 7000;
/// Max characters of partial digests merged in one reduce step.
pub const REDUCE_INPUT_CHARS: usize = 9000;
/// Max time for one model call.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(300);
/// Index `kind` of digest chunks.
pub const DIGEST_KIND: &str = "digest";

/// Cut `text` into windows of at most `size` chars on paragraph (then line,
/// then hard) boundaries.
pub fn windows(text: &str, size: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0usize;
    let push_piece = |piece: &str, out: &mut Vec<String>, cur: &mut String, cur_len: &mut usize| {
        let n = piece.chars().count();
        if *cur_len > 0 && *cur_len + n + 2 > size {
            out.push(std::mem::take(cur).trim().to_string());
            *cur_len = 0;
        }
        if !cur.is_empty() {
            cur.push_str("\n\n");
            *cur_len += 2;
        }
        cur.push_str(piece);
        *cur_len += n;
    };
    for para in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        if para.chars().count() <= size {
            push_piece(para, &mut out, &mut cur, &mut cur_len);
            continue;
        }
        // An oversized paragraph: by lines, then hard cuts.
        for line in para.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let chars: Vec<char> = line.chars().collect();
            for piece in chars.chunks(size) {
                let piece: String = piece.iter().collect();
                push_piece(&piece, &mut out, &mut cur, &mut cur_len);
            }
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Whether most letters of `text` are Arabic.
pub fn is_arabic(text: &str) -> bool {
    let (mut ar, mut lat) = (0usize, 0usize);
    for c in text.chars().take(20_000) {
        if matches!(c, '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}') {
            ar += 1;
        } else if c.is_ascii_alphabetic() {
            lat += 1;
        }
    }
    ar > 0 && ar >= lat
}

const MAP_AR: &str = "أنت محلل مستندات دقيق. ستُعطى نص مستند (أو جزءًا منه). اكتب ملخصًا منظمًا \
باللغة العربية فقط، من النص المعطى فقط، دون أي معلومة من خارجه، بهذه العناوين بالضبط:\n\
## نظرة عامة\n(جملتان أو ثلاث: موضوع النص وغرضه ولمن يُوجَّه)\n\
## هيكل المستند\n(عناوين الأقسام/المواد/الفصول كما وردت بأرقامها، مع سطر قصير عن محتوى كل منها)\n\
## الحقائق والأرقام الأساسية\n(كل رقم ومبلغ ونسبة وتاريخ ومدة واسم ورقم مادة، منقولًا حرفيًا مع ما يخصه)\n\
## الشروط والاستثناءات\n(من يستحق ماذا، ومتى، وما المستثنى)\n\
## التعريفات\n(المصطلحات المعرَّفة في النص، إن وجدت)\n\
اكتب نقاطًا قصيرة دقيقة. لا تخترع ولا تستنتج ما ليس في النص، وانقل الأرقام كما هي. \
إذا لم يوجد محتوى لعنوان فاكتب تحته «—».";

const MAP_EN: &str = "You are a precise document analyst. You will be given a document (or part \
of one). Write a structured digest in English only, from the given text only, with no outside \
information, using exactly these headings:\n\
## Overview\n(2–3 sentences: what the text is about, its purpose and audience)\n\
## Document structure\n(the section/article/chapter titles as written, with their numbers, one short line each)\n\
## Key facts and numbers\n(every number, amount, percentage, date, duration, name and article number, quoted exactly with what it refers to)\n\
## Conditions and exceptions\n(who is entitled to what, when, and what is excluded)\n\
## Definitions\n(terms the text defines, if any)\n\
Use short, precise bullet points. Do not invent or infer anything that is not in the text; \
copy numbers exactly. If a heading has no content, write \"—\" under it.";

const REDUCE_AR: &str = "ستُعطى ملخصات منظمة لأجزاء متتالية من مستند واحد. ادمجها في ملخص واحد \
منظم باللغة العربية بالعناوين نفسها (نظرة عامة، هيكل المستند، الحقائق والأرقام الأساسية، الشروط \
والاستثناءات، التعريفات). اجعل النظرة العامة من 3 إلى 5 جمل عن المستند كله. احذف التكرار فقط، \
واحتفظ بكل رقم وتاريخ واسم ورقم مادة كما ورد حرفيًا. لا تضف أي معلومة ليست في الملخصات.";

const REDUCE_EN: &str = "You will be given structured digests of consecutive parts of one \
document. Merge them into one structured digest in English with the same headings (Overview, \
Document structure, Key facts and numbers, Conditions and exceptions, Definitions). Make the \
overview 3–5 sentences about the whole document. Remove only repetition; keep every number, \
date, name and article number exactly as written. Add nothing that is not in the digests.";

/// Group partial digests so each group is ≤ `max` chars (at least one per group).
pub fn reduce_groups(parts: &[String], max: usize) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut size = 0usize;
    for p in parts {
        let n = p.chars().count();
        match groups.last_mut() {
            Some(g) if size + n <= max => {
                g.push(p.clone());
                size += n;
            }
            _ => {
                groups.push(vec![p.clone()]);
                size = n;
            }
        }
    }
    groups
}

/// Title shown at the top of every digest chunk (helps "what is X about?").
fn doc_title(name: &str) -> String {
    let stem = match name.rsplit_once('.') {
        Some((s, ext)) if ext.len() <= 5 && !s.is_empty() => s,
        _ => name,
    };
    stem.replace(['_', '-'], " ").trim().to_string()
}

/// Digest chunks: the digest cut by the structural chunker, each prefixed
/// with the document title.
pub fn digest_chunks(doc_name: &str, digest: &str) -> Vec<String> {
    let title = doc_title(doc_name);
    chunk::chunk(digest)
        .into_iter()
        .map(|c| format!("{title}\n{c}"))
        .collect()
}

/// Generation settings for one document's digest.
pub struct DigestJob<'a> {
    pub model: &'a str,
    pub num_ctx: u32,
    pub cancelled: &'a (dyn Fn() -> bool + Send + Sync),
}

impl AssistantStore {
    fn digest_file(&self, id: &str, doc_id: &str) -> std::path::PathBuf {
        self.dir_of(id)
            .join("docs")
            .join(format!("{doc_id}.digest.md"))
    }

    /// The digest text of a document, if any.
    pub fn digest_text(&self, id: &str, doc_id: &str) -> anyhow::Result<Option<String>> {
        if !Self::valid_id(id) || !Self::valid_id(doc_id) {
            return Ok(None);
        }
        match std::fs::read_to_string(self.digest_file(id, doc_id)) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub(super) fn remove_digest_file(&self, id: &str, doc_id: &str) {
        if Self::valid_id(doc_id) {
            let _ = std::fs::remove_file(self.digest_file(id, doc_id));
        }
    }

    /// Map-reduce the document text into a digest with the chat model.
    async fn generate_digest(
        &self,
        ollama_url: &str,
        doc_name: &str,
        text: &str,
        job: &DigestJob<'_>,
        progress: &mut (impl FnMut(Progress) + Send),
    ) -> anyhow::Result<String> {
        let arabic = is_arabic(text);
        let (map_sys, reduce_sys) = if arabic {
            (MAP_AR, REDUCE_AR)
        } else {
            (MAP_EN, REDUCE_EN)
        };
        let wins = windows(text, WINDOW_CHARS);
        if wins.is_empty() {
            bail!("no text to digest");
        }
        // Estimated steps: windows + merges (a partial digest ≈ 2,500 chars).
        let est_reduce = |n: usize| {
            let mut steps = 0;
            let mut parts = n;
            while parts > 1 {
                let groups = (parts * 2500).div_ceil(REDUCE_INPUT_CHARS).max(1);
                let groups = if groups >= parts { parts - 1 } else { groups };
                steps += groups;
                parts = groups;
            }
            steps
        };
        let mut total = wins.len() + est_reduce(wins.len());
        let mut done = 0usize;
        progress(Progress {
            status: "digesting",
            done,
            total,
        });
        let call = |system: &'static str, user: String, num_predict: i32| async move {
            let req = llm::ChatRequest {
                model: job.model,
                system,
                user: &user,
                num_ctx: job.num_ctx,
                num_predict,
                temperature: 0.2,
                timeout: CALL_TIMEOUT,
            };
            llm::chat(&self.inner.http, ollama_url, &req, job.cancelled).await
        };
        let mut parts = Vec::with_capacity(wins.len());
        let n = wins.len();
        for (k, w) in wins.into_iter().enumerate() {
            let user = if n == 1 {
                format!("«{doc_name}»\n\n{w}")
            } else if arabic {
                format!("«{doc_name}» — الجزء {} من {n}:\n\n{w}", k + 1)
            } else {
                format!("\"{doc_name}\" — part {} of {n}:\n\n{w}", k + 1)
            };
            let out = call(map_sys, user, if n == 1 { 1800 } else { 1100 }).await?;
            if out.trim().is_empty() {
                bail!("the model returned an empty digest");
            }
            parts.push(out);
            done += 1;
            progress(Progress {
                status: "digesting",
                done,
                total,
            });
        }
        while parts.len() > 1 {
            let mut groups = reduce_groups(&parts, REDUCE_INPUT_CHARS);
            if groups.len() == parts.len() {
                // Every part is large: merge pairs anyway so it converges.
                groups = parts.chunks(2).map(<[String]>::to_vec).collect();
            }
            total = total.max(done + groups.len());
            let mut next = Vec::with_capacity(groups.len());
            for g in groups {
                if g.len() == 1 {
                    next.extend(g);
                    continue;
                }
                let user = g
                    .iter()
                    .enumerate()
                    .map(|(i, p)| format!("### {}\n{p}", i + 1))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let user = format!("«{doc_name}»\n\n{user}");
                let out = call(reduce_sys, user, 1800).await?;
                if out.trim().is_empty() {
                    bail!("the model returned an empty digest");
                }
                next.push(out);
                done += 1;
                progress(Progress {
                    status: "digesting",
                    done,
                    total: total.max(done),
                });
            }
            parts = next;
        }
        Ok(parts.pop().unwrap_or_default())
    }

    /// Replace the digest chunks of `doc_id` in the index with `digest`'s
    /// (embedded now); `None` just removes them. Returns the chunk count.
    async fn store_digest_chunks(
        &self,
        ollama_url: &str,
        a: &Assistant,
        doc_id: &str,
        doc_name: &str,
        digest: Option<&str>,
    ) -> anyhow::Result<usize> {
        let pieces = digest
            .map(|d| digest_chunks(doc_name, d))
            .unwrap_or_default();
        let idx = self.load_index(&a.id).await?;
        let vecs = if pieces.is_empty() {
            Vec::new()
        } else {
            super::embed::embed_batched(&self.inner.http, ollama_url, &a.embed_model, &pieces)
                .await?
        };
        if let Some(v) = vecs.first()
            && idx.dim != 0
            && v.len() != idx.dim
        {
            bail!("embedding size changed; re-index the knowledge base");
        }
        let mut chunks: Vec<IndexedChunk> = idx
            .chunks
            .iter()
            .filter(|c| !(c.doc_id == doc_id && c.kind.is_some()))
            .cloned()
            .collect();
        super::check_chunk_total(chunks.len() + pieces.len())?;
        let n = pieces.len();
        chunks.extend(
            pieces
                .into_iter()
                .zip(vecs)
                .enumerate()
                .map(|(i, (text, vec))| IndexedChunk {
                    doc_id: doc_id.to_string(),
                    i,
                    kind: Some(DIGEST_KIND.into()),
                    text,
                    vec,
                }),
        );
        let dim = chunks.first().map_or(idx.dim, |c| c.vec.len());
        self.save_index(
            &a.id,
            Index {
                version: if idx.chunks.is_empty() {
                    index::INDEX_VERSION
                } else {
                    idx.version
                },
                embed_model: if idx.chunks.is_empty() {
                    a.embed_model.clone()
                } else {
                    idx.embed_model.clone()
                },
                dim,
                chunks,
            },
        )
        .await?;
        Ok(n)
    }

    /// Generate (or regenerate) the digest of one document while holding the
    /// assistant's lock taken by the caller. A failure is recorded in
    /// `DigestMeta` (status `"failed"`) and also returned.
    pub(super) async fn digest_locked(
        &self,
        ollama_url: &str,
        a: &mut Assistant,
        doc_id: &str,
        job: &DigestJob<'_>,
        progress: &mut (impl FnMut(Progress) + Send),
    ) -> anyhow::Result<DigestMeta> {
        let Some(pos) = a.docs.iter().position(|d| d.id == doc_id) else {
            return Err(NotFound("document").into());
        };
        let name = a.docs[pos].name.clone();
        let text = self.doc_text(&a.id, doc_id)?.ok_or(NotFound("document"))?;
        let started = Instant::now();
        let result = async {
            let digest = self
                .generate_digest(ollama_url, &name, &text, job, progress)
                .await?;
            if (job.cancelled)() {
                return Err(llm::Cancelled.into());
            }
            progress(Progress {
                status: "embedding",
                done: 0,
                total: 1,
            });
            write_atomic(&self.digest_file(&a.id, doc_id), digest.as_bytes())?;
            let n = self
                .store_digest_chunks(ollama_url, a, doc_id, &name, Some(&digest))
                .await?;
            anyhow::Ok((digest, n))
        }
        .await;
        let meta = match &result {
            Ok((digest, n)) => DigestMeta {
                status: "ok".into(),
                model: job.model.to_string(),
                chars: digest.chars().count(),
                chunks: *n,
                created_at: now_ms(),
                millis: Some(started.elapsed().as_millis() as u64),
                error: None,
            },
            Err(e) => {
                // Keep a previous good digest (file + chunks) if there is one.
                if let Some(prev) = a.docs[pos].digest.clone()
                    && prev.status != "failed"
                    && self.digest_text(&a.id, doc_id)?.is_some()
                {
                    prev
                } else {
                    DigestMeta {
                        status: "failed".into(),
                        model: job.model.to_string(),
                        created_at: now_ms(),
                        error: Some(format!("{e:#}")),
                        ..Default::default()
                    }
                }
            }
        };
        a.docs[pos].digest = Some(meta.clone());
        a.updated_at = now_ms();
        self.write(a)?;
        progress(Progress {
            status: "saving",
            done: 1,
            total: 1,
        });
        match result {
            Ok(_) => Ok(meta),
            Err(e) => Err(e),
        }
    }

    /// Generate digests for one document (`doc_id`) or for every document
    /// without a good one (`force` regenerates all). Calls `on_doc` after each
    /// document with its result. Stops early when `job.cancelled()`.
    #[allow(clippy::too_many_arguments)]
    pub async fn digest_documents(
        &self,
        ollama_url: &str,
        id: &str,
        doc_id: Option<&str>,
        force: bool,
        job: &DigestJob<'_>,
        mut progress: impl FnMut(Progress) + Send,
        mut on_doc: impl FnMut(&str, &str, anyhow::Result<DigestMeta>) + Send,
    ) -> anyhow::Result<Assistant> {
        let lock = self.lock_for(id);
        let _guard = lock.lock().await;
        let mut a = self.require(id)?;
        let targets: Vec<(String, String)> = match doc_id {
            Some(d) => {
                let doc = a
                    .docs
                    .iter()
                    .find(|x| x.id == d)
                    .ok_or(NotFound("document"))?;
                vec![(doc.id.clone(), doc.name.clone())]
            }
            None => a
                .docs
                .iter()
                .filter(|d| force || d.digest.as_ref().is_none_or(|g| g.status == "failed"))
                .map(|d| (d.id.clone(), d.name.clone()))
                .collect(),
        };
        for (did, name) in targets {
            if (job.cancelled)() {
                break;
            }
            let r = self
                .digest_locked(ollama_url, &mut a, &did, job, &mut progress)
                .await;
            on_doc(&did, &name, r);
        }
        Ok(a)
    }

    /// Save a digest edited by the user and re-embed its chunks.
    pub async fn save_digest(
        &self,
        ollama_url: &str,
        id: &str,
        doc_id: &str,
        text: &str,
    ) -> anyhow::Result<super::DocMeta> {
        let lock = self.lock_for(id);
        let _guard = lock.lock().await;
        let mut a = self.require(id)?;
        let Some(pos) = a.docs.iter().position(|d| d.id == doc_id) else {
            return Err(NotFound("document").into());
        };
        let name = a.docs[pos].name.clone();
        let text = text.trim();
        if text.is_empty() {
            self.remove_digest_file(id, doc_id);
            self.store_digest_chunks(ollama_url, &a, doc_id, &name, None)
                .await?;
            a.docs[pos].digest = None;
        } else {
            super::check_doc_text_size(&name, text.len())?;
            let n = self
                .store_digest_chunks(ollama_url, &a, doc_id, &name, Some(text))
                .await?;
            write_atomic(&self.digest_file(id, doc_id), text.as_bytes())?;
            let prev = a.docs[pos].digest.clone().unwrap_or_default();
            a.docs[pos].digest = Some(DigestMeta {
                status: "edited".into(),
                chars: text.chars().count(),
                chunks: n,
                created_at: now_ms(),
                millis: None,
                error: None,
                model: prev.model,
            });
        }
        a.updated_at = now_ms();
        self.write(&a)?;
        Ok(a.docs[pos].clone())
    }
}

/// Deep-mode `num_ctx` for digest calls: the assistant's answer context, so
/// Ollama doesn't reload the model between digesting and chatting.
pub fn digest_num_ctx(a: &Assistant, app: &deep::AppDefaults) -> u32 {
    deep::effective_options(a, app).0.max(8192)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_respect_size_and_paragraphs() {
        let para = "word ".repeat(300); // 1500 chars
        let text = [para.trim(); 10].join("\n\n");
        let w = windows(&text, 7000);
        assert_eq!(w.len(), 3); // 4 + 4 + 2 paragraphs
        assert!(w.iter().all(|x| x.chars().count() <= 7000));
        assert_eq!(w.concat().matches("word").count(), 3000);
        // A single huge line is hard-cut.
        let w = windows(&"x".repeat(15_000), 7000);
        assert_eq!(w.len(), 3);
        assert!(windows("", 7000).is_empty());
        assert_eq!(windows("short", 7000), vec!["short".to_string()]);
    }

    #[test]
    fn reduce_groups_pack_and_always_progress() {
        let p = |n: usize| "a".repeat(n);
        let g = reduce_groups(&[p(3000), p(3000), p(3000), p(3000)], 9000);
        assert_eq!(g.iter().map(Vec::len).collect::<Vec<_>>(), vec![3, 1]);
        let g = reduce_groups(&[p(10_000), p(10_000)], 9000);
        assert_eq!(g.len(), 2); // caller then merges pairs
    }

    #[test]
    fn language_and_titles() {
        assert!(is_arabic("دليل الموظف 2024 Employee"));
        assert!(!is_arabic("Employee handbook 2024"));
        assert_eq!(
            doc_title("employee_handbook-2024.pdf"),
            "employee handbook 2024"
        );
        assert_eq!(doc_title("notes"), "notes");
        let c = digest_chunks("دليل.pdf", "## نظرة عامة\nدليل الموظف.");
        assert_eq!(c.len(), 1);
        assert!(c[0].starts_with("دليل\n"));
    }
}
