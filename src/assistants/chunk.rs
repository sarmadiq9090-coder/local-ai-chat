//! Split document text into chunks for embedding.
//!
//! Structure first, size second (see `docs/specs/assistants.md`):
//! 1. **Sections.** A Markdown heading (`#` … `######`) starts a section. In a
//!    document without headings every blank-line paragraph is its own section,
//!    so the topics of a small multi-topic file (an FAQ, a price list) are never
//!    blended into one diluted embedding. Sections shorter than
//!    [`MIN_SECTION`] characters (a lone heading, a one-line question) are
//!    merged with the next one (the last one with the previous).
//! 2. **Units.** Inside a section the units are its paragraphs; a paragraph
//!    longer than [`MAX_CHUNK`] is split into its lines (list items, PDF
//!    lines), and a line still longer than that is cut by [`chunk_with`]
//!    (paragraph → sentence → space → hard cut, with overlap).
//! 3. **Packing.** Units of the same section are packed together while the
//!    chunk stays within [`MAX_CHUNK`] characters (target ~400–600). A chunk
//!    that continues a section repeats the section heading, and one that
//!    continues a split paragraph starts with up to [`CHUNK_OVERLAP`]
//!    characters of the previous chunk's tail. Chunks never cross sections.
//!
//! Sizes are in characters (not bytes), so Arabic and emoji never get cut
//! inside a UTF-8 sequence.

/// Size used when a single long line has to be cut.
pub const CHUNK_SIZE: usize = 500;
/// Upper bound for packing units into one chunk (before a repeated heading).
pub const MAX_CHUNK: usize = 600;
/// Approximate overlap between consecutive pieces of one paragraph.
pub const CHUNK_OVERLAP: usize = 100;
/// Sections shorter than this are merged with a neighbour.
pub const MIN_SECTION: usize = 80;

fn len(s: &str) -> usize {
    s.chars().count()
}

fn is_heading(line: &str) -> bool {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') && line.len() > hashes + 1
}

/// One topic: an optional heading plus its paragraphs (the heading is also
/// the first paragraph).
#[derive(Debug, Default)]
struct Section {
    heading: Option<String>,
    paras: Vec<String>,
}

impl Section {
    fn chars(&self) -> usize {
        self.paras.iter().map(|p| len(p)).sum()
    }
}

fn sections(text: &str) -> Vec<Section> {
    let has_headings = text.lines().any(|l| is_heading(l.trim()));
    let mut out: Vec<Section> = Vec::new();
    let mut cur = Section::default();
    let mut para: Vec<&str> = Vec::new();

    let flush_para = |para: &mut Vec<&str>, cur: &mut Section| {
        if !para.is_empty() {
            cur.paras.push(para.join("\n"));
            para.clear();
        }
    };
    let flush_section = |cur: &mut Section, out: &mut Vec<Section>| {
        if !cur.paras.is_empty() {
            out.push(std::mem::take(cur));
        }
    };

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush_para(&mut para, &mut cur);
            if !has_headings {
                flush_section(&mut cur, &mut out);
            }
        } else if has_headings && is_heading(line) {
            flush_para(&mut para, &mut cur);
            flush_section(&mut cur, &mut out);
            cur.heading = Some(line.to_string());
            cur.paras.push(line.to_string());
        } else {
            para.push(line);
        }
    }
    flush_para(&mut para, &mut cur);
    flush_section(&mut cur, &mut out);
    merge_tiny(out)
}

/// Merge sections shorter than [`MIN_SECTION`] into the next one (a tiny last
/// one into the previous, if that stays within [`MAX_CHUNK`]).
fn merge_tiny(secs: Vec<Section>) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut carry: Option<Section> = None;
    for mut s in secs {
        if let Some(c) = carry.take() {
            let mut paras = c.paras;
            paras.append(&mut s.paras);
            s.paras = paras;
            s.heading = s.heading.or(c.heading);
        }
        if s.chars() < MIN_SECTION {
            carry = Some(s);
        } else {
            out.push(s);
        }
    }
    if let Some(c) = carry {
        match out.last_mut() {
            Some(prev) if prev.chars() + c.chars() <= MAX_CHUNK => prev.paras.extend(c.paras),
            _ => out.push(c),
        }
    }
    out
}

/// How a unit attaches to the previous one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Joint {
    /// First unit of a paragraph.
    Para,
    /// Next line of a paragraph that was split into lines.
    Line,
    /// Piece of a long line cut by [`chunk_with`] (already overlapping).
    Piece,
}

fn units(sec: &Section) -> Vec<(Joint, String)> {
    let mut out = Vec::new();
    for p in &sec.paras {
        if len(p) <= MAX_CHUNK {
            out.push((Joint::Para, p.clone()));
            continue;
        }
        for (li, line) in p.lines().enumerate() {
            let first = if li == 0 { Joint::Para } else { Joint::Line };
            if len(line) <= MAX_CHUNK {
                out.push((first, line.to_string()));
            } else {
                for (pi, piece) in chunk_with(line, CHUNK_SIZE, CHUNK_OVERLAP)
                    .into_iter()
                    .enumerate()
                {
                    out.push((if pi == 0 { first } else { Joint::Piece }, piece));
                }
            }
        }
    }
    out
}

/// The last ~`n` characters of `s`, starting at a word boundary.
fn tail(s: &str, n: usize) -> &str {
    let total = len(s);
    if total <= n {
        return s;
    }
    let start = s.char_indices().nth(total - n).map_or(0, |(i, _)| i);
    let t = &s[start..];
    match t.find(char::is_whitespace) {
        Some(p) if p + 1 < t.len() => t[p..].trim_start(),
        _ => t,
    }
}

/// Chunk with the structural rules described in the module docs.
pub fn chunk(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for sec in sections(text) {
        let mut cur = String::new();
        let mut last_unit = String::new();
        for (joint, unit) in units(&sec) {
            let sep = if joint == Joint::Para { "\n\n" } else { "\n" };
            if !cur.is_empty() && len(&cur) + sep.len() + len(&unit) <= MAX_CHUNK {
                cur.push_str(sep);
                cur.push_str(&unit);
            } else {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    if let Some(h) = &sec.heading
                        && !unit.starts_with(h.as_str())
                    {
                        cur.push_str(h);
                        cur.push('\n');
                    }
                    if joint == Joint::Line {
                        cur.push_str(tail(&last_unit, CHUNK_OVERLAP));
                        cur.push('\n');
                    }
                }
                cur.push_str(&unit);
            }
            last_unit = unit;
        }
        if !cur.trim().is_empty() {
            out.push(cur);
        }
    }
    out
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '؟' | '۔' | '\n')
}

/// Cut `text` into pieces of at most `size` characters, overlapping by about
/// `overlap` characters. Used for single lines too long for one chunk.
/// Empty/whitespace-only pieces are dropped.
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
    if let Some(i) = window
        .clone()
        .rev()
        .find(|&i| i >= 1 && chars[i] == '\n' && chars[i - 1] == '\n')
    {
        return i + 1;
    }
    // 2. Sentence end followed by whitespace (so "3.14" or "a.b" don't split).
    if let Some(i) = window.clone().rev().find(|&i| {
        is_sentence_end(chars[i])
            && (chars[i] == '\n' || chars.get(i + 1).is_none_or(|c| c.is_whitespace()))
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

    /// Longest allowed chunk: packed units plus a repeated heading/overlap line.
    const BOUND: usize = MAX_CHUNK + 2 * CHUNK_OVERLAP;

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(chunk("  hello world  "), vec!["hello world"]);
        assert!(chunk("   \n\n ").is_empty());
        assert!(chunk("").is_empty());
    }

    #[test]
    fn long_line_respects_size_and_covers_text() {
        let sentence = "The quick brown fox jumps over the lazy dog. ";
        let text = sentence.repeat(100);
        let chunks = chunk(&text);
        assert!(chunks.len() > 6);
        for c in &chunks {
            assert!(len(c) <= MAX_CHUNK, "too long: {}", len(c));
            assert!(c.ends_with('.'), "chunk should end at a sentence: {c:?}");
        }
        assert!(chunks.last().unwrap().ends_with("lazy dog."));
    }

    #[test]
    fn consecutive_pieces_overlap() {
        let text: String = (0..400).map(|i| format!("word{i} ")).collect();
        let chunks = chunk_with(&text, 200, 50);
        for pair in chunks.windows(2) {
            let first_word_next = pair[1].split_whitespace().next().unwrap();
            assert!(
                pair[0].contains(first_word_next),
                "no overlap between {pair:?}"
            );
            assert!(first_word_next.starts_with("word"));
        }
        // Same through `chunk` for one huge line.
        let chunks = chunk(&text);
        for pair in chunks.windows(2) {
            let w = pair[1].split_whitespace().next().unwrap();
            assert!(pair[0].contains(w), "no overlap between {pair:?}");
        }
    }

    #[test]
    fn small_multi_topic_file_is_split_by_paragraph() {
        let text = "Our return policy: items can be returned within 14 days with the receipt, unused and in the original box.\n\n\
                    Shipping inside Baghdad takes one day and costs 5,000 IQD; other provinces take three days.\n\n\
                    Working hours are 9am to 9pm every day except Friday, when the shop opens at 2pm instead.";
        let chunks = chunk(text);
        assert_eq!(chunks.len(), 3, "{chunks:#?}");
        assert!(chunks[0].starts_with("Our return policy"));
        assert!(chunks[2].contains("Friday"));
    }

    #[test]
    fn tiny_paragraphs_merge_with_the_next() {
        let text = "س: ما هي سياسة الإرجاع؟\n\nج: يمكن إرجاع المنتجات خلال 14 يومًا من الشراء مع الفاتورة، بشرط أن تكون غير مستعملة وفي علبتها الأصلية.\n\n\
                    س: كم تكلفة الشحن؟\n\nج: الشحن داخل بغداد بيوم واحد وبتكلفة 5,000 دينار، وباقي المحافظات خلال ثلاثة أيام.";
        let chunks = chunk(text);
        assert_eq!(chunks.len(), 2, "{chunks:#?}");
        assert!(chunks[0].contains("الإرجاع") && chunks[0].contains("14 يومًا"));
        assert!(chunks[1].contains("الشحن") && !chunks[1].contains("الإرجاع"));
    }

    #[test]
    fn headings_start_sections_and_repeat_on_continuations() {
        let body = "- item with a few words in it to take some room here\n".repeat(30);
        let text = format!(
            "# Prices\n\nCoffee is 3,000 IQD and tea is 1,000 IQD at every branch of the cafe in the city.\n\n## Menu\n\n{body}\n## Contact\n\nCall 0770 123 4567 or write to hello@example.com for bookings and events."
        );
        let chunks = chunk(&text);
        assert!(chunks[0].starts_with("# Prices") && !chunks[0].contains("## Menu"));
        let menu: Vec<_> = chunks.iter().filter(|c| c.contains("item with")).collect();
        assert!(menu.len() > 1);
        assert!(menu.iter().all(|c| c.starts_with("## Menu")), "{menu:#?}");
        // Overlap: a continuation repeats the previous chunk's last line.
        let last_line = menu[0].lines().last().unwrap();
        assert!(menu[1].lines().nth(1) == Some(last_line));
        assert!(chunks.last().unwrap().starts_with("## Contact"));
        for c in &chunks {
            assert!(len(c) <= BOUND, "too long: {}", len(c));
        }
    }

    #[test]
    fn lone_heading_merges_into_next_section() {
        let text = "# Title\n\n## Section A\n\nThis section has enough text to stand on its own as a separate chunk, really.";
        let chunks = chunk(text);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].starts_with("# Title") && chunks[0].contains("Section A"));
    }

    #[test]
    fn arabic_sentences_and_boundaries() {
        let sentence = "هذه جملة عربية طويلة نسبيا تحتوي على كلمات كثيرة؟ ";
        let text = sentence.repeat(60);
        let chunks = chunk(&text);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(len(c) <= MAX_CHUNK);
            assert!(
                c.ends_with('؟'),
                "should cut after Arabic question mark: {c:?}"
            );
        }
        let urdu = "یہ ایک جملہ ہے۔ ".repeat(100);
        assert!(chunk(&urdu).iter().all(|c| c.ends_with('۔')));
    }

    #[test]
    fn pdf_like_lines_are_packed_with_overlap() {
        let text: String = (0..60)
            .map(|i| format!("سطر رقم {i} من ملف PDF طويل بدون فراغات بين الفقرات\n"))
            .collect();
        let chunks = chunk(&text);
        assert!(chunks.len() > 3);
        for pair in chunks.windows(2) {
            let first = pair[1].lines().next().unwrap();
            assert!(pair[0].ends_with(first), "no overlap: {pair:?}");
        }
        for c in &chunks {
            assert!(len(c) <= BOUND);
        }
    }

    #[test]
    fn no_panic_on_multibyte_without_spaces() {
        let emoji = "😀🎉👍🏽".repeat(500);
        let arabic = "مرحبا".repeat(500);
        for text in [emoji, arabic, "x".repeat(5000)] {
            let chunks = chunk(&text);
            assert!(!chunks.is_empty());
            for c in &chunks {
                assert!(len(c) <= MAX_CHUNK);
            }
        }
        assert!(!chunk_with("abc def ghi ﷺ 😀", 1, 100).is_empty());
        assert_eq!(tail("ﷺ😀 abc", 2), "bc");
    }

    #[test]
    fn decimal_points_do_not_split() {
        let text = format!("{} value 3.14159 end", "word ".repeat(150));
        for c in chunk_with(&text, 760, 0) {
            assert!(!c.ends_with("3."));
        }
    }
}
