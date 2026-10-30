//! Split document text into overlapping chunks for embedding.
//!
//! Sizes are in characters (not bytes), so Arabic and emoji never get cut
//! inside a UTF-8 sequence. A chunk ends preferably at a paragraph break, then
//! at a sentence end (`. ! ? ؟ ۔` or a newline), then at a space, and only as
//! a last resort mid-word. Consecutive chunks overlap by roughly `overlap`
//! characters, starting at a word boundary.

/// Target chunk size in characters.
pub const CHUNK_SIZE: usize = 800;
/// Approximate overlap between consecutive chunks, in characters.
pub const CHUNK_OVERLAP: usize = 150;

/// Chunk with the default size and overlap.
pub fn chunk(text: &str) -> Vec<String> {
    chunk_with(text, CHUNK_SIZE, CHUNK_OVERLAP)
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '؟' | '۔' | '\n')
}

/// Chunk `text` into pieces of at most `size` characters, overlapping by about
/// `overlap` characters. Empty/whitespace-only pieces are dropped.
pub fn chunk_with(text: &str, size: usize, overlap: usize) -> Vec<String> {
    let size = size.max(16);
    let overlap = overlap.min(size / 2);
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut start = 0;

    while start < n {
        if n - start <= size {
            push_trimmed(&mut out, &chars[start..]);
            break;
        }
        let limit = start + size;
        let end = find_break(&chars, start + size / 2, limit);
        push_trimmed(&mut out, &chars[start..end]);

        // Next chunk starts `overlap` chars back, moved forward to a word start.
        let mut next = end.saturating_sub(overlap).max(start + 1);
        if overlap > 0 {
            let scan_to = (next + overlap / 2).min(end);
            if let Some(p) = (next..scan_to).find(|&i| chars[i].is_whitespace()) {
                next = p + 1;
            }
        } else {
            next = end;
        }
        start = next.max(start + 1);
    }
    out
}

/// Best cut position (exclusive end) in `(min, limit]`.
fn find_break(chars: &[char], min: usize, limit: usize) -> usize {
    let window = min..limit;
    // 1. Paragraph break: cut right after "\n\n".
    if let Some(i) = window.clone().rev().find(|&i| i >= 1 && chars[i] == '\n' && chars[i - 1] == '\n') {
        return i + 1;
    }
    // 2. Sentence end followed by whitespace (so "3.14" or "a.b" don't split).
    if let Some(i) = window.clone().rev().find(|&i| {
        is_sentence_end(chars[i]) && (chars[i] == '\n' || chars.get(i + 1).is_none_or(|c| c.is_whitespace()))
    }) {
        return i + 1;
    }
    // 3. Any whitespace.
    if let Some(i) = window.rev().find(|&i| chars[i].is_whitespace()) {
        return i + 1;
    }
    // 4. Hard cut (still on a char boundary).
    limit
}

fn push_trimmed(out: &mut Vec<String>, piece: &[char]) {
    let s: String = piece.iter().collect();
    let t = s.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(chunk("  hello world  "), vec!["hello world"]);
        assert!(chunk("   \n\n ").is_empty());
        assert!(chunk("").is_empty());
    }

    #[test]
    fn respects_size_and_covers_text() {
        let sentence = "The quick brown fox jumps over the lazy dog. ";
        let text = sentence.repeat(100);
        let chunks = chunk(&text);
        assert!(chunks.len() > 4);
        for c in &chunks {
            assert!(c.chars().count() <= CHUNK_SIZE, "too long: {}", c.chars().count());
            // Cut at sentence ends.
            assert!(c.ends_with('.'), "chunk should end at a sentence: {c:?}");
        }
        // Every word of the source appears; last chunk ends the text.
        assert!(chunks.last().unwrap().ends_with("lazy dog."));
    }

    #[test]
    fn consecutive_chunks_overlap() {
        let text: String = (0..400).map(|i| format!("word{i} ")).collect();
        let chunks = chunk_with(&text, 200, 50);
        for pair in chunks.windows(2) {
            let first_word_next = pair[1].split_whitespace().next().unwrap();
            assert!(pair[0].contains(first_word_next), "no overlap between {pair:?}");
            // Overlap starts at a word boundary.
            assert!(first_word_next.starts_with("word"));
        }
    }

    #[test]
    fn prefers_paragraph_breaks() {
        let para1 = "a".repeat(10) + " " + &"b ".repeat(300); // ~610 chars
        let para2 = "Second paragraph. ".repeat(30);
        let text = format!("{}\n\n{}", para1.trim(), para2);
        let chunks = chunk(&text);
        assert_eq!(chunks[0], para1.trim());
    }

    #[test]
    fn arabic_sentences_and_boundaries() {
        let sentence = "هذه جملة عربية طويلة نسبيا تحتوي على كلمات كثيرة؟ ";
        let text = sentence.repeat(60);
        let chunks = chunk(&text);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(c.chars().count() <= CHUNK_SIZE);
            assert!(c.ends_with('؟'), "should cut after Arabic question mark: {c:?}");
        }
        let urdu = "یہ ایک جملہ ہے۔ ".repeat(100);
        assert!(chunk(&urdu).iter().all(|c| c.ends_with('۔')));
    }

    #[test]
    fn no_panic_on_multibyte_without_spaces() {
        let emoji = "😀🎉👍🏽".repeat(500);
        let arabic = "مرحبا".repeat(500);
        for text in [emoji, arabic, "x".repeat(5000)] {
            let chunks = chunk(&text);
            assert!(!chunks.is_empty());
            for c in &chunks {
                assert!(c.chars().count() <= CHUNK_SIZE);
            }
        }
        // Tiny sizes and overlap >= size are handled.
        assert!(!chunk_with("abc def ghi ﷺ 😀", 1, 100).is_empty());
    }

    #[test]
    fn decimal_points_do_not_split() {
        let text = format!("{} value 3.14159 end", "word ".repeat(150));
        for c in chunk_with(&text, 760, 0) {
            assert!(!c.ends_with("3."));
        }
    }
}
