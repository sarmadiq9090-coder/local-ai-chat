//! Vector index scoring: cosine similarity (dot product of unit vectors) plus a
//! cheap keyword-overlap boost, then top-k above a minimum score.
//!
//! The keyword boost lets exact names, codes and numbers match even when the
//! embedding similarity is mediocre. Arabic text is lightly normalized for the
//! keyword comparison (diacritics removed, alef/ya/ta-marbuta forms unified).

use std::collections::HashSet;

use super::types::Index;

/// Version of the chunking/index format. Bump it when [`super::chunk`] or the
/// scoring changes in a way that needs new chunks: older indexes report
/// `stale` and are rebuilt. 2 = structural chunker (v2.1.1).
pub const INDEX_VERSION: u32 = 2;

/// Default number of passages returned.
pub const DEFAULT_K: usize = 5;
/// Default minimum combined score (cosine + keyword boost).
///
/// Tuned on `bge-m3` with `tests::retrieval_eval_live` (AR/EN questions vs an
/// Arabic FAQ, an English handbook, a dense 550-char Arabic policy paragraph
/// and an off-topic recipe, chunked by [`super::chunk`]). Best scores:
///
/// | question → knowledge                         | relevant | best other |
/// |----------------------------------------------|----------|------------|
/// | AR → AR FAQ («ما هي سياسة الإرجاع؟»)          | 0.71–0.81 | ≤ 0.56    |
/// | EN → AR FAQ ("What is the return policy?")   | 0.65–0.67 | ≤ 0.54    |
/// | AR → EN handbook («كم بدل النقل الشهري؟»)     | 0.61–0.69 | ≤ 0.67    |
/// | EN → EN handbook                             | 0.80–0.82 | ≤ 0.57    |
/// | EN → dense AR paragraph ("overtime rate?")   | 0.50–0.60 | ≤ 0.64    |
/// | unrelated (capital of France, World Cup, poem)| –        | 0.27–0.38 |
/// | near-miss (chocolate cake, dollar rate)       | –        | 0.43–0.45 |
///
/// The old single-chunk-per-file index scored the same questions 0.43–0.49,
/// which the former 0.5 threshold rejected. 0.42 keeps every relevant hit
/// with margin and drops clearly unrelated questions; near-misses may pass,
/// and in strict mode the model then answers "not in knowledge".
pub const MIN_SCORE: f32 = 0.42;
/// Results scoring more than this below the best result are dropped, so a
/// clear hit isn't padded with loosely related chunks (same size as the
/// maximum keyword boost, so a boosted hit can't push out a semantic one).
pub const RELATIVE_WINDOW: f32 = 0.15;
/// Score added when every query keyword appears in the chunk (scaled by the
/// fraction of keywords found).
pub const KEYWORD_BOOST: f32 = 0.15;

/// Dot product (= cosine similarity for unit vectors).
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Fold a char for keyword matching: lowercase, unify Arabic letter variants;
/// `None` drops it (Arabic diacritics and tatweel).
fn fold(c: char) -> Option<char> {
    match c {
        '\u{064B}'..='\u{0652}' | '\u{0670}' | '\u{0640}' => None,
        'أ' | 'إ' | 'آ' | 'ٱ' => Some('ا'),
        'ى' => Some('ي'),
        'ة' => Some('ه'),
        'ؤ' => Some('و'),
        'ئ' => Some('ي'),
        // Arabic-Indic digits → ASCII.
        '٠'..='٩' => char::from_digit(c as u32 - '٠' as u32, 10),
        '۰'..='۹' => char::from_digit(c as u32 - '۰' as u32, 10),
        _ => c.to_lowercase().next(),
    }
}

/// Keywords of a text: alphanumeric words, folded; words shorter than 2 chars
/// are skipped unless they contain a digit.
pub fn keywords(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut word = String::new();
    let mut flush = |w: &mut String| {
        if w.chars().count() >= 2 || w.chars().any(|c| c.is_ascii_digit()) {
            out.insert(std::mem::take(w));
        } else {
            w.clear();
        }
    };
    for c in text.chars() {
        if c.is_alphanumeric() || ('\u{064B}'..='\u{0652}').contains(&c) || c == '\u{0640}' {
            if let Some(f) = fold(c) {
                word.push(f);
            }
        } else if !word.is_empty() {
            flush(&mut word);
        }
    }
    if !word.is_empty() {
        flush(&mut word);
    }
    out
}

/// Boost in `[0, KEYWORD_BOOST]`: the fraction of query keywords present in the chunk.
pub fn keyword_boost(query_kw: &HashSet<String>, chunk_text: &str) -> f32 {
    if query_kw.is_empty() {
        return 0.0;
    }
    let chunk_kw = keywords(chunk_text);
    let hits = query_kw.iter().filter(|k| chunk_kw.contains(*k)).count();
    KEYWORD_BOOST * hits as f32 / query_kw.len() as f32
}

/// Indices into `index.chunks` with their combined score, best first:
/// `score = cosine + keyword_boost`, keeping at most `k` with `score >= min_score`.
pub fn top_k(
    index: &Index,
    query_vec: &[f32],
    query: &str,
    k: usize,
    min_score: f32,
) -> Vec<(usize, f32)> {
    let qkw = keywords(query);
    let scored: Vec<(usize, f32)> = index
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| c.vec.len() == query_vec.len())
        .map(|(i, c)| (i, dot(&c.vec, query_vec) + keyword_boost(&qkw, &c.text)))
        .collect();
    top_k_from(scored, k, min_score)
}

/// Best first; keep at most `k` with `score >= min_score` and within
/// [`RELATIVE_WINDOW`] of the best score.
pub fn top_k_from(mut scored: Vec<(usize, f32)>, k: usize, min_score: f32) -> Vec<(usize, f32)> {
    scored.retain(|&(_, s)| s >= min_score);
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    if let Some(&(_, best)) = scored.first() {
        scored.retain(|&(_, s)| s >= best - RELATIVE_WINDOW);
    }
    scored.truncate(k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistants::embed::normalize;
    use crate::assistants::types::IndexedChunk;

    fn chunk(text: &str, mut vec: Vec<f32>) -> IndexedChunk {
        normalize(&mut vec);
        IndexedChunk {
            doc_id: "d".into(),
            i: 0,
            kind: None,
            text: text.into(),
            vec,
        }
    }

    fn index() -> Index {
        Index {
            version: INDEX_VERSION,
            embed_model: "m".into(),
            dim: 3,
            chunks: vec![
                chunk("alpha", vec![1.0, 0.0, 0.0]),
                chunk("beta", vec![0.9, 0.1, 0.0]),
                chunk("gamma", vec![0.0, 1.0, 0.0]),
                chunk("delta", vec![0.5, 0.5, 0.0]),
                chunk("wrong dim", vec![1.0, 0.0]),
            ],
        }
    }

    #[test]
    fn ordering_threshold_and_k() {
        let idx = index();
        // Query between alpha and delta: alpha 0.92, beta 0.95, delta 0.92, gamma 0.38.
        let q = [0.92, 0.38, 0.0];
        let r = top_k(&idx, &q, "", 10, 0.35);
        let order: Vec<usize> = r.iter().map(|x| x.0).collect();
        // gamma is within the threshold but far below the best; wrong dim skipped.
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], 1);
        assert!(!order.contains(&2) && !order.contains(&4));
        assert!(r.windows(2).all(|w| w[0].1 >= w[1].1));
        assert_eq!(top_k(&idx, &q, "", 2, 0.35).len(), 2);
        assert!(top_k(&idx, &q, "", 5, 1.01).is_empty());
    }

    #[test]
    fn relative_window_drops_far_results() {
        let scored = vec![(0, 0.80), (1, 0.70), (2, 0.66), (3, 0.50), (4, 0.30)];
        let r = top_k_from(scored, 10, 0.42);
        assert_eq!(r, vec![(0, 0.80), (1, 0.70), (2, 0.66)]);
        // Weak best: everything near it stays, the threshold still applies.
        let r = top_k_from(vec![(0, 0.45), (1, 0.43), (2, 0.40)], 10, 0.42);
        assert_eq!(r, vec![(0, 0.45), (1, 0.43)]);
        assert!(top_k_from(vec![(0, 0.2)], 10, 0.42).is_empty());
    }

    #[test]
    fn keyword_boost_lifts_exact_match() {
        let idx = index();
        let q = [0.0, 1.0, 0.0];
        // gamma 1.0; alpha 0 (+0.15 boost) stays far below the best.
        assert!(top_k(&idx, &q, "alpha", 5, 0.1).iter().all(|x| x.0 != 0));
        // Alone (no strong hit), the boost alone can lift it over a low threshold.
        let r = top_k_from(
            vec![(0, keyword_boost(&keywords("alpha"), "alpha"))],
            5,
            0.1,
        );
        assert_eq!(r.len(), 1);
        // Boost reorders near-ties: "beta" (0.994) + boost beats "alpha" (1.0).
        let r = top_k(&idx, &[1.0, 0.0, 0.0], "beta", 5, 0.35);
        assert_eq!(r[0].0, 1);
    }

    #[test]
    fn keywords_fold_arabic_and_numbers() {
        let kw = keywords("ما هي عاصِمةُ «فرنسا»؟ رقم ٤٢ و A-7");
        assert!(kw.contains("عاصمه"));
        assert!(kw.contains("فرنسا"));
        assert!(kw.contains("42"));
        assert!(kw.contains("7"));
        assert!(!kw.contains("و")); // single letter dropped
        let q = keywords("أحمد 2024");
        assert!((keyword_boost(&q, "كتب احمد هذا في عام 2024.") - KEYWORD_BOOST).abs() < 1e-6);
        assert!((keyword_boost(&q, "Ahmed in 2024") - KEYWORD_BOOST / 2.0).abs() < 1e-6);
        assert_eq!(keyword_boost(&HashSet::new(), "anything"), 0.0);
    }
}
