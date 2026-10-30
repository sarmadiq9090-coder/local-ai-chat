//! Vector index scoring: cosine similarity (dot product of unit vectors) plus a
//! cheap keyword-overlap boost, then top-k above a minimum score.
//!
//! The keyword boost lets exact names, codes and numbers match even when the
//! embedding similarity is mediocre. Arabic text is lightly normalized for the
//! keyword comparison (diacritics removed, alef/ya/ta-marbuta forms unified).

use std::collections::HashSet;

use super::types::Index;

/// Default number of passages returned.
pub const DEFAULT_K: usize = 5;
/// Default minimum combined score.
// Tuned on bge-m3: unrelated text scores ~0.35, same meaning across AR/EN ~0.88.
pub const MIN_SCORE: f32 = 0.5;
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
pub fn top_k(index: &Index, query_vec: &[f32], query: &str, k: usize, min_score: f32) -> Vec<(usize, f32)> {
    let qkw = keywords(query);
    let mut scored: Vec<(usize, f32)> = index
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| c.vec.len() == query_vec.len())
        .map(|(i, c)| (i, dot(&c.vec, query_vec) + keyword_boost(&qkw, &c.text)))
        .filter(|&(_, s)| s >= min_score)
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
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
        IndexedChunk { doc_id: "d".into(), i: 0, text: text.into(), vec }
    }

    fn index() -> Index {
        Index {
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
        let q = [1.0, 0.0, 0.0];
        let r = top_k(&idx, &q, "", 10, 0.35);
        let order: Vec<usize> = r.iter().map(|x| x.0).collect();
        assert_eq!(order, vec![0, 1, 3]); // gamma (0.0) below threshold, wrong dim skipped
        assert!((r[0].1 - 1.0).abs() < 1e-6);
        assert!(r.windows(2).all(|w| w[0].1 >= w[1].1));
        assert_eq!(top_k(&idx, &q, "", 2, 0.35).len(), 2);
        assert!(top_k(&idx, &q, "", 5, 1.01).is_empty());
    }

    #[test]
    fn keyword_boost_lifts_exact_match() {
        let idx = index();
        let q = [1.0, 0.0, 0.0];
        // "gamma" has cosine 0 but matches the keyword: 0 + 0.15 < 0.35 still filtered.
        assert!(top_k(&idx, &q, "gamma", 5, 0.35).iter().all(|x| x.0 != 2));
        // With a lower threshold it shows up thanks to the boost.
        let r = top_k(&idx, &q, "gamma", 5, 0.1);
        assert!(r.iter().any(|x| x.0 == 2));
        // Boost reorders near-ties: "beta" (0.994) + boost beats "alpha" (1.0).
        let r = top_k(&idx, &q, "beta", 5, 0.35);
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
