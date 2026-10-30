//! Plain-text extraction for knowledge documents.
//!
//! Text-like files are decoded as UTF-8 (lossy); PDFs go through the
//! `pdf-extract` crate on a blocking thread. Everything else is rejected with
//! "unsupported file type". The result is normalized (no NULs, `\n` line
//! endings, collapsed runs of spaces and blank lines).
//!
//! **Arabic PDFs.** Many PDF writers store Arabic as shaped glyphs in visual
//! (left-to-right) order, so `pdf-extract` returns presentation forms
//! (U+FB50–U+FDFF, U+FE70–U+FEFF) with every line reversed, plus U+FFFD for
//! glyphs it can't map. [`fix_arabic_pdf`] repairs such lines: it reverses the
//! word order (keeping runs of numbers/Latin words in their own order),
//! reverses the characters of Arabic words, maps presentation forms to base
//! letters (NFKC), glues stray one-letter fragments back onto their word and
//! drops U+FFFD. The result is readable but can still contain words split by
//! a space (glyph gaps after ر ز و …), so such documents carry the warning
//! [`WARN_ARABIC_PDF`].

use anyhow::{Context, bail};
use unicode_normalization::UnicodeNormalization;

/// Warning recorded on a document whose Arabic PDF text had to be repaired
/// (`DocMeta.warning`); the UI suggests a .docx/.txt copy instead.
pub const WARN_ARABIC_PDF: &str = "arabic_pdf_quality";
/// Warning for a PDF with almost no text per page: most likely scanned images
/// (no OCR here); the UI suggests uploading an OCR'd copy.
pub const WARN_SCANNED_PDF: &str = "scanned_pdf";
/// Below this many characters of text per page a PDF counts as scanned.
pub const SCANNED_CHARS_PER_PAGE: usize = 80;

/// Extracted text plus an optional quality warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub text: String,
    pub warning: Option<&'static str>,
    /// Page count (PDFs only).
    pub pages: Option<u32>,
}

/// `Some(WARN_SCANNED_PDF)` when a PDF has fewer than
/// [`SCANNED_CHARS_PER_PAGE`] characters of text per page on average.
pub fn scanned_warning(chars: usize, pages: Option<u32>) -> Option<&'static str> {
    let pages = pages.filter(|&p| p > 0)? as usize;
    (chars < pages * SCANNED_CHARS_PER_PAGE).then_some(WARN_SCANNED_PDF)
}

/// Extensions decoded as UTF-8 text.
const TEXT_EXTS: &[&str] = &[
    "txt", "md", "csv", "json", "xml", "yml", "yaml", "toml", "html", "css", "js", "ts", "py",
    "rs", "go", "java", "c", "cpp", "h", "cs", "php", "rb", "sh", "ps1", "sql", "ini", "log",
];

/// Lowercased extension of a file name (`"Report.PDF"` → `"pdf"`).
fn extension(name: &str) -> Option<String> {
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

/// Whether a file with this name can be added to a knowledge base.
pub fn is_supported(name: &str) -> bool {
    extension(name).is_some_and(|e| e == "pdf" || TEXT_EXTS.contains(&e.as_str()))
}

/// Extract normalized plain text from an uploaded file.
pub async fn extract(name: &str, bytes: Vec<u8>) -> anyhow::Result<Extracted> {
    let ext = extension(name).unwrap_or_default();
    if ext == "pdf" {
        let (raw, pages) = tokio::task::spawn_blocking(move || {
            let pages = pdf_extract::Document::load_mem(&bytes)
                .ok()
                .map(|d| d.get_pages().len() as u32);
            pdf_extract::extract_text_from_mem(&bytes).map(|t| (t, pages))
        })
        .await
        .context("PDF extraction failed")?
        .map_err(|e| anyhow::anyhow!("could not read PDF: {e}"))?;
        let (text, warning) = fix_arabic_pdf(&raw);
        let text = pdf_paragraphs(&normalize(&text));
        let warning = scanned_warning(text.chars().count(), pages).or(warning);
        Ok(Extracted {
            text,
            warning,
            pages,
        })
    } else if TEXT_EXTS.contains(&ext.as_str()) {
        Ok(Extracted {
            text: normalize(&String::from_utf8_lossy(&bytes)),
            warning: None,
            pages: None,
        })
    } else {
        bail!("unsupported file type: {name}");
    }
}

fn is_presentation_form(c: char) -> bool {
    matches!(c, '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFE}')
}

fn is_arabic(c: char) -> bool {
    matches!(c, '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}') || is_presentation_form(c)
}

/// Arabic combining marks (harakat, superscript alef).
fn is_mark(c: char) -> bool {
    matches!(c, '\u{064B}'..='\u{065F}' | '\u{0670}')
}

/// Strong left-to-right content: Latin (and other non-Arabic) letters and
/// digits, including Arabic-Indic digits (a number keeps its LTR order).
fn is_ltr(c: char) -> bool {
    c.is_ascii_digit()
        || matches!(c, '\u{0660}'..='\u{0669}' | '\u{06F0}'..='\u{06F9}')
        || (c.is_alphabetic() && !is_arabic(c))
}

fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        _ => c,
    }
}

/// Map presentation forms to base letters (NFKC on those chars only, so other
/// text such as `²` or `ﬁ` is left alone). Farsi-yeh forms, which some fonts
/// use for the Arabic ي, become ي; heh-doachashmee forms (U+FBAA–U+FBAD, NFKC
/// would give Urdu ھ), which some fonts use for the Arabic ه, become ه.
fn unshape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\u{FBFC}'..='\u{FBFF}') {
            out.push('ي');
        } else if matches!(c, '\u{FBAA}'..='\u{FBAD}' | '\u{06BE}') {
            out.push('ه');
        } else if is_presentation_form(c) {
            out.extend(c.to_string().nfkc());
        } else {
            out.push(c);
        }
    }
    out
}

/// Contextual shape of a presentation form.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Form {
    Isolated,
    Final,
    Initial,
    Medial,
}

/// Shape of an Arabic presentation form (U+FE80–U+FEFC, Farsi yeh
/// U+FBFC–U+FBFF); `None` for base letters and anything else.
fn form(c: char) -> Option<Form> {
    // (first code point, number of forms): forms are ordered isolated, final,
    // initial, medial.
    const GROUPS: &[(u32, u32)] = &[
        (0xFE80, 1),
        (0xFE81, 2),
        (0xFE83, 2),
        (0xFE85, 2),
        (0xFE87, 2),
        (0xFE89, 4),
        (0xFE8D, 2),
        (0xFE8F, 4),
        (0xFE93, 2),
        (0xFE95, 4),
        (0xFE99, 4),
        (0xFE9D, 4),
        (0xFEA1, 4),
        (0xFEA5, 4),
        (0xFEA9, 2),
        (0xFEAB, 2),
        (0xFEAD, 2),
        (0xFEAF, 2),
        (0xFEB1, 4),
        (0xFEB5, 4),
        (0xFEB9, 4),
        (0xFEBD, 4),
        (0xFEC1, 4),
        (0xFEC5, 4),
        (0xFEC9, 4),
        (0xFECD, 4),
        (0xFED1, 4),
        (0xFED5, 4),
        (0xFED9, 4),
        (0xFEDD, 4),
        (0xFEE1, 4),
        (0xFEE5, 4),
        (0xFEE9, 4),
        (0xFEED, 2),
        (0xFEEF, 2),
        (0xFEF1, 4),
        (0xFEF5, 2),
        (0xFEF7, 2),
        (0xFEF9, 2),
        (0xFEFB, 2),
        (0xFBFC, 4),
    ];
    let cp = c as u32;
    let &(start, _) = GROUPS.iter().find(|&&(s, n)| cp >= s && cp < s + n)?;
    Some(match cp - start {
        0 => Form::Isolated,
        1 => Form::Final,
        2 => Form::Initial,
        _ => Form::Medial,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Rtl,
    Ltr,
    Neutral,
}

/// A word in logical order, with how it attaches to its neighbours.
struct Word {
    text: String,
    /// Its first letter is joined to the previous letter (final/medial form),
    /// or it is a stray isolated one-letter fragment: append to the previous word.
    glue_prev: bool,
    /// Its last letter is joined to the next letter (initial/medial form), or
    /// it is the lone conjunction و: prepend to the next word.
    glue_next: bool,
}

fn rtl_word(visual_token: &str) -> Word {
    let logical: Vec<char> = visual_token.chars().rev().map(mirror).collect();
    let letters: Vec<char> = logical
        .iter()
        .copied()
        .filter(|&c| is_arabic(c) && !is_mark(c))
        .collect();
    let first = logical
        .iter()
        .copied()
        .find(|&c| is_arabic(c) && !is_mark(c));
    let last = logical
        .iter()
        .copied()
        .rev()
        .find(|&c| is_arabic(c) && !is_mark(c));
    let first_form = first.and_then(form);
    let last_form = last.and_then(form);
    let single = letters.len() == 1 && logical.iter().all(|&c| is_arabic(c));
    let text = unshape(&logical.iter().collect::<String>());
    let conj = single && text.trim_matches(is_mark) == "و";
    Word {
        glue_prev: matches!(first_form, Some(Form::Final | Form::Medial))
            || (single && !conj && matches!(first_form, None | Some(Form::Isolated))),
        glue_next: matches!(last_form, Some(Form::Initial | Form::Medial)) || conj,
        text,
    }
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, '\u{0660}'..='\u{0669}' | '\u{06F0}'..='\u{06F9}')
}

/// Punctuation that sticks to numbers: common separators (`. , : /`, also
/// inside a number), `-`/`–` (ranges, phone numbers) and `%`.
fn is_num_punct(c: char) -> bool {
    matches!(c, '.' | ',' | ':' | '/' | '-' | '–' | '%' | '٫' | '٬' | '٪')
}

/// Separators that stay inside a number when they sit between two digits
/// (`12.5`, `3,450`, `8:00`, `15/03/2026`).
fn is_num_sep(c: char) -> bool {
    matches!(c, '.' | ',' | ':' | '/' | '٫' | '٬')
}

/// A token made only of digits and number punctuation, with a digit.
fn is_numeric_token(t: &str) -> bool {
    t.chars().any(is_digit) && t.chars().all(|c| is_digit(c) || is_num_punct(c))
}

/// A glyph-positioned fragment of a number: one digit, optionally preceded by
/// one punctuation mark (`5`, `.5`, `-0`, `%1`, `,4`, `:0`, `/0`).
fn is_digit_fragment(t: &str) -> bool {
    let cs: Vec<char> = t.chars().collect();
    match cs.as_slice() {
        [d] => is_digit(*d),
        [p, d] => is_num_punct(*p) && is_digit(*d),
        _ => false,
    }
}

/// Glyph-positioned PDFs (e.g. printed by Edge/Chrome) can put a space
/// between every digit of a number (`.1 0 0 1 1`, `%1 2 .5`, `3 ,4 5 0`).
/// Re-join runs of two or more one-digit fragments into one token.
fn merge_digit_fragments<'a>(tokens: &[&'a str]) -> Vec<std::borrow::Cow<'a, str>> {
    use std::borrow::Cow;
    let mut out: Vec<Cow<'a, str>> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let mut j = i;
        while j < tokens.len() && is_digit_fragment(tokens[j]) {
            j += 1;
        }
        if j - i >= 2 {
            out.push(Cow::Owned(tokens[i..j].concat()));
            i = j;
        } else {
            out.push(Cow::Borrowed(tokens[i]));
            i += 1;
        }
    }
    out
}

/// Logical order of a run of numbers in an Arabic line, given in visual
/// order (`.10011`, `% 12.5`, `4242 -0800`). Digits in Arabic context are
/// Arabic numbers: each number (digits joined by `. , : /`) keeps its own
/// order, but the numbers and the marks around them are laid out right to
/// left, so the run is reversed number by number. Then sentence/number marks
/// are glued to the number before them (`10011.`, `12.5%`) and a `-` touching
/// a number on one side loses the extractor's space on the other
/// (`0800-4242`).
fn logical_number_run(visual: &str) -> String {
    let cs: Vec<char> = visual.chars().collect();
    let mut atoms: Vec<String> = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        if is_digit(cs[i]) {
            let mut s = String::new();
            while i < cs.len()
                && (is_digit(cs[i])
                    || (is_num_sep(cs[i]) && cs.get(i + 1).is_some_and(|&c| is_digit(c))))
            {
                s.push(cs[i]);
                i += 1;
            }
            atoms.push(s);
        } else {
            atoms.push(cs[i].to_string());
            i += 1;
        }
    }
    atoms.reverse();
    let is_core = |a: &str| a.starts_with(is_digit);
    let mut out: Vec<String> = Vec::with_capacity(atoms.len());
    for (k, a) in atoms.iter().enumerate() {
        if a == " " {
            let prev = out.last().map(String::as_str).unwrap_or("");
            let next = atoms.get(k + 1).map(String::as_str).unwrap_or("");
            let after_next = atoms.get(k + 2).map(String::as_str).unwrap_or("");
            let prev_prev = out
                .len()
                .checked_sub(2)
                .map(|p| out[p].as_str())
                .unwrap_or("");
            let mark = |s: &str| matches!(s, "." | "," | ":" | "%" | "٪" | "٫" | "٬");
            let dash = |s: &str| matches!(s, "-" | "–");
            // "12.5 %" → "12.5%", "4242 ." → "4242."
            let drop = (is_core(prev) && mark(next))
                // "0800- 4242" → "0800-4242" (dash glued to a number on the left)
                || (dash(prev) && is_core(prev_prev) && is_core(next))
                // "0800 -4242" → "0800-4242" (dash glued to a number on the right)
                || (is_core(prev) && dash(next) && is_core(after_next));
            if drop {
                continue;
            }
        }
        out.push(a.clone());
    }
    out.concat()
}

/// Rebuild one visual-order line in logical order.
fn fix_visual_line(line: &str) -> String {
    let raw_tokens: Vec<&str> = line.split_whitespace().collect();
    let tokens = merge_digit_fragments(&raw_tokens);
    let numeric: Vec<bool> = tokens.iter().map(|t| is_numeric_token(t)).collect();
    let dirs: Vec<Dir> = tokens
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if t.chars().any(is_arabic) {
                Dir::Rtl
            } else if t.chars().any(is_ltr) {
                Dir::Ltr
            } else if t.chars().all(is_num_punct)
                && ((i > 0 && numeric[i - 1]) || numeric.get(i + 1).copied().unwrap_or(false))
            {
                // "% 12.5", ". 4242": a mark next to a number belongs to it.
                Dir::Ltr
            } else {
                Dir::Neutral
            }
        })
        .collect();
    // Neutrals between two LTR tokens ("8:00 ~ 4:00") stay in the LTR run.
    let resolved: Vec<Dir> = (0..tokens.len())
        .map(|i| match dirs[i] {
            Dir::Neutral => {
                let prev = dirs[..i].iter().rev().find(|d| **d != Dir::Neutral);
                let next = dirs[i + 1..].iter().find(|d| **d != Dir::Neutral);
                if prev == Some(&Dir::Ltr) && next == Some(&Dir::Ltr) {
                    Dir::Ltr
                } else {
                    Dir::Rtl
                }
            }
            d => d,
        })
        .collect();
    // Consecutive LTR tokens form one run; each RTL token is its own group.
    // The groups are then reversed. A run with Latin letters keeps its own
    // order; a run of numbers only is laid out right to left number by number
    // (see `logical_number_run`).
    let mut groups: Vec<(bool, Vec<Word>)> = Vec::new();
    for (t, d) in tokens.iter().zip(&resolved) {
        if *d == Dir::Ltr {
            let w = Word {
                text: t.to_string(),
                glue_prev: false,
                glue_next: false,
            };
            match groups.last_mut() {
                Some((true, run)) => run.push(w),
                _ => groups.push((true, vec![w])),
            }
        } else {
            groups.push((false, vec![rtl_word(t)]));
        }
    }
    for (ltr, run) in groups.iter_mut() {
        let latin = run
            .iter()
            .any(|w| w.text.chars().any(|c| c.is_alphabetic() && !is_arabic(c)));
        if *ltr && !latin {
            let visual = run
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            *run = vec![Word {
                text: logical_number_run(&visual),
                glue_prev: false,
                glue_next: false,
            }];
        }
    }
    groups.reverse();
    let mut words: Vec<Word> = groups.into_iter().flat_map(|(_, w)| w).collect();

    // A lam-alef ligature (ﻻ ﻷ …) can come out as a lone «ل» before a word
    // starting with alef («العمل اء» ← «العم ل اء», «ا ل أعضاء»): the «ل»
    // belongs to the next word, and a lone «ا» before it is the article.
    for i in 0..words.len().saturating_sub(1) {
        let next_alef = words[i + 1].text.starts_with(['ا', 'أ', 'إ', 'آ']);
        if words[i].text == "ل" && next_alef {
            words[i].glue_next = true;
        }
        if words[i].text == "ا" && words[i + 1].text == "ل" {
            words[i].glue_prev = false;
            words[i].glue_next = true;
        }
    }

    // Re-join words that the PDF split at glyph gaps.
    let ends_arabic = |s: &str| s.chars().last().is_some_and(|c| is_arabic(c) || is_mark(c));
    let starts_arabic = |s: &str| s.chars().next().is_some_and(is_arabic);
    let mut out: Vec<String> = Vec::new();
    let mut pending_next = false;
    for w in words {
        let join = match out.last() {
            Some(prev) => {
                (pending_next && starts_arabic(&w.text)) || (w.glue_prev && ends_arabic(prev))
            }
            None => false,
        };
        if join {
            out.last_mut().expect("checked").push_str(&w.text);
        } else {
            out.push(w.text);
        }
        pending_next = w.glue_next;
    }
    out.join(" ")
}

/// Repair Arabic text that a PDF stored in visual order with presentation
/// forms (see the module docs). Text without presentation forms is returned
/// unchanged (apart from dropping U+FFFD). Returns [`WARN_ARABIC_PDF`] when
/// lines had to be rebuilt or many glyphs were lost.
pub fn fix_arabic_pdf(raw: &str) -> (String, Option<&'static str>) {
    let arabic = raw.chars().filter(|&c| is_arabic(c)).count();
    let shaped = raw.chars().filter(|&c| is_presentation_form(c)).count();
    let lost = raw.chars().filter(|&c| c == '\u{FFFD}').count();
    // A writer that stores shaped glyphs does so for the whole document: treat
    // every Arabic line as visual order (a line may hold only unshaped ا ر و).
    let visual_doc = arabic > 0 && shaped * 5 >= arabic;
    let mut fixed_lines = 0usize;
    let lines: Vec<String> = raw
        .split('\n')
        .map(|line| {
            let line: String = line.chars().filter(|&c| c != '\u{FFFD}').collect();
            let has_arabic = line.chars().any(is_arabic);
            if has_arabic && (visual_doc || line.chars().any(is_presentation_form)) {
                fixed_lines += 1;
                fix_visual_line(&line)
            } else {
                line
            }
        })
        .collect();
    let letters = raw.chars().filter(|c| c.is_alphanumeric()).count().max(1);
    let warning = (fixed_lines > 0 || lost * 100 > letters).then_some(WARN_ARABIC_PDF);
    (lines.join("\n"), warning)
}

/// `pdf-extract` often separates *every* line with a blank line, which would
/// make each PDF line its own paragraph (and chunk). When no two text lines
/// are adjacent, rebuild paragraphs: lines are joined with `\n`, and a new
/// paragraph starts at a heading-like line (short, no sentence-ending
/// punctuation). Other text is returned unchanged.
pub fn pdf_paragraphs(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let adjacent = lines
        .windows(2)
        .filter(|w| !w[0].is_empty() && !w[1].is_empty())
        .count();
    let nonempty: Vec<&str> = lines.iter().copied().filter(|l| !l.is_empty()).collect();
    if adjacent > 0 || nonempty.len() < 3 {
        return text.to_string();
    }
    let heading_like = |l: &str| {
        l.chars().count() <= 40 && !l.ends_with(['.', '!', '?', '؟', '۔', ':', '،', ',', ';', '؛'])
    };
    let mut out = String::with_capacity(text.len());
    for (i, line) in nonempty.iter().enumerate() {
        if i > 0 {
            out.push_str(if heading_like(line) { "\n\n" } else { "\n" });
        }
        out.push_str(line);
    }
    out
}

/// Strip NULs/BOM and control chars, unify line endings, collapse runs of
/// horizontal whitespace to one space, trim lines, and keep at most one blank
/// line between paragraphs.
pub fn normalize(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0usize;
    for line in text.split('\n') {
        let mut clean = String::with_capacity(line.len());
        let mut in_space = false;
        for c in line.chars() {
            if c == '\0' || c == '\u{feff}' {
                continue;
            }
            if c.is_whitespace() || c.is_control() {
                in_space = true;
                continue;
            }
            if in_space && !clean.is_empty() {
                clean.push(' ');
            }
            in_space = false;
            clean.push(c);
        }
        if clean.is_empty() {
            blank_run += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank_run > 0 { "\n\n" } else { "\n" });
        }
        blank_run = 0;
        out.push_str(&clean);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Raw lines of a headless-Edge PDF (Segoe UI 11pt) where pdf-extract puts
    /// a space between every digit, and the number each must yield.
    const EDGE_SPLIT_DIGITS: &[(&str, &str)] = &[
        (".1 0 0 1 1  ﻮﻫ يﺪﻳﺮ ﺒﻟا  ﺰ ﻣﺮ ﻟا", "هو 10011."),
        (".2 5 0 0 0  عﻮﻓ ﺪﻤﻟا  لﺎ ﻤﻟا  سأ ر", " 25000."),
        (".2 0 2 6  مﺎ ﻋ ﻲﻓ  ﺔﻛ رشﻟا  ﺖﺴﺳﺄ ﺗ", "في عام 2026."),
        (
            ".ءﺎ ﻀ ﻋأ ل ا  ﻞﻜﻟ %1 2 .5  يﻮﻨﺴﻟا  ﻢﺼ ﺨﻟا  ﺔﺒﺴﻧ",
            "السنوي 12.5% لكل الأعضاء.",
        ),
        (
            ".ا ًل ﻴ ﻤﻋ 3 ,4 5 0  ﻦﻴ ﻠﺠﺴﻤﻟا  ءا ل ﻤﻌﻟا  د ﺪﻋ",
            "العملاء المسجلين 3,450 ",
        ),
        (
            ".ﺎ ًﻧﺎ ﺠﻣ 4 2 4 2 -0 8 0 0  ءا ل ﻤﻌﻟا  ﺔﻣﺪﺧ  ﻢﻗ ر",
            "خدمة العملاء 0800-4242 مجانًا.",
        ),
        (
            ".ﻂﻘ ﻓ  %2 0  ﺔﺒﺴﻨﻟا و 1 5 /0 3 /2 0 2 6  ﺦ ﻳرﺎ ﺘﻟا و 4 :0 0  ﻰﻟإ  8 :0 0  ﻦﻣ ما وﺪﻟا",
            "من 8:00 إلى 4:00 والتاريخ 15/03/2026 والنسبة 20% فقط.",
        ),
        (
            ".3 ,4 5 0  مﻮﺳﺮ ﻟا و .4 2 4 2 -0 8 0 0  ﺐﺘﻜﻤﻟا  ﻒﺗ ﺎ ﻫ",
            "المكتب 0800-4242. والرسوم 3,450.",
        ),
    ];

    /// The same sentences printed with Arial: whole numbers, but marks next
    /// to them are separate or attached on the visual side.
    const EDGE_ARIAL: &[(&str, &str)] = &[
        (".10011  وھ يدﯾرﺑﻟا زﻣرﻟا", "الرمز البريدي هو 10011."),
        (".25000  عوﻓدﻣﻟا لﺎﻣﻟا سأر", "رأس المال المدفوع 25000."),
        (".2026  مﺎﻋ ﻲﻓ ﺔﻛرﺷﻟا تﺳﺳﺄﺗ", "تأسست الشركة في عام 2026."),
        (
            ".ءﺎﺿﻋﻷا لﻛﻟ % 12.5  يوﻧﺳﻟا مﺻﺧﻟا ﺔﺑﺳﻧ",
            "نسبة الخصم السنوي 12.5% لكل الأعضاء.",
        ),
        (
            ". ًﻼﯾﻣﻋ 3,450  نﯾﻠﺟﺳﻣﻟا ءﻼﻣﻌﻟا ددﻋ",
            "عدد العملاء المسجلين 3,450 عميلاً .",
        ),
        (
            ".ﺎًﻧﺎﺟﻣ 4242 -0800  ءﻼﻣﻌﻟا ﺔﻣدﺧ مﻗر",
            "رقم خدمة العملاء 0800-4242 مجانًا.",
        ),
        (
            ".طﻘﻓ % 20  ﺔﺑﺳﻧﻟاو  15/03/2026  ﺦﯾرﺎﺗﻟاو  4:00  ﻰﻟإ  8:00  نﻣ ماودﻟا",
            "الدوام من 8:00 إلى 4:00 والتاريخ 15/03/2026 والنسبة 20% فقط.",
        ),
        (
            ".3,450  موﺳرﻟاو . 4242 -0800  بﺗﻛﻣﻟا فﺗﺎھ",
            "هاتف المكتب 0800-4242. والرسوم 3,450.",
        ),
    ];

    #[test]
    fn edge_pdf_split_digits_become_whole_numbers() {
        for (raw, want) in EDGE_SPLIT_DIGITS {
            let (text, warn) = fix_arabic_pdf(raw);
            assert_eq!(warn, Some(WARN_ARABIC_PDF));
            assert!(text.contains(want), "{raw:?} → {text:?}, want {want:?}");
        }
    }

    #[test]
    fn edge_pdf_numbers_keep_logical_order_and_punctuation() {
        for (raw, want) in EDGE_ARIAL {
            assert_eq!(fix_arabic_pdf(raw).0, *want, "{raw:?}");
        }
    }

    #[test]
    fn number_runs_in_visual_order() {
        assert_eq!(logical_number_run(".10011"), "10011.");
        assert_eq!(logical_number_run("% 12.5"), "12.5%");
        assert_eq!(logical_number_run("%20"), "20%");
        assert_eq!(logical_number_run("4242 -0800"), "0800-4242");
        assert_eq!(logical_number_run(". 4242 -0800"), "0800-4242.");
        assert_eq!(logical_number_run("3,450"), "3,450");
        // A spaced range keeps its spaces; two numbers swap (RTL layout).
        assert_eq!(logical_number_run("20 - 10"), "10 - 20");
        assert_eq!(logical_number_run("4:00 8:00"), "8:00 4:00");
        // Fragments are merged only when every one is a single digit.
        let toks = ["2.1", "3", "4", "5", "x"];
        assert_eq!(merge_digit_fragments(&toks), vec!["2.1", "345", "x"]);
        let toks = ["مدتها", "4", "5", "دقيقة"];
        assert_eq!(merge_digit_fragments(&toks), vec!["مدتها", "45", "دقيقة"]);
        let toks = ["30", "يومًا", "و", "60"];
        assert_eq!(merge_digit_fragments(&toks), vec!["30", "يومًا", "و", "60"]);
    }

    #[test]
    fn latin_text_with_numbers_is_untouched() {
        for s in [
            "Plain English 12.5% and 0800-4242. Total: 3,450 on 15/03/2026.",
            "Version 2.1 3 of Windows 11 (build 26100)",
        ] {
            assert_eq!(fix_arabic_pdf(s), (s.to_string(), None));
        }
        // A Latin run inside an Arabic visual line keeps its order.
        let (text, _) = fix_arabic_pdf("ﻡﺪﺨﺘﺳﺍ Windows 11 ﻞﻴﻐﺸﺗ");
        assert_eq!(text, "تشغيل Windows 11 استخدم");
    }

    #[test]
    fn heh_doachashmee_becomes_arabic_heh() {
        // «لأهلية» with a heh-doachashmee medial form, and «شهادة» with the Urdu ھ.
        assert_eq!(
            unshape("\u{FEDF}\u{FE84}\u{FBAC}\u{FEE0}\u{FEF4}\u{FE94}"),
            "لأهلية"
        );
        assert_eq!(unshape("ش\u{06BE}ادة"), "شهادة");
    }

    #[test]
    fn normalize_whitespace_and_nuls() {
        let s = "\u{feff}Hello\0  \t world  \r\n\r\n\r\n\nمرحبا   بكم\rline";
        assert_eq!(normalize(s), "Hello world\n\nمرحبا بكم\nline");
    }

    #[test]
    fn supported_types() {
        assert!(is_supported("a.TXT"));
        assert!(is_supported("كتاب.pdf"));
        assert!(is_supported("x.ps1"));
        assert!(!is_supported("photo.png"));
        assert!(!is_supported("noext"));
    }

    #[tokio::test]
    async fn rejects_unsupported() {
        let err = extract("a.exe", vec![0, 1, 2]).await.unwrap_err();
        assert!(err.to_string().contains("unsupported file type"));
        let ok = extract("a.md", "# عنوان\n\nنص".as_bytes().to_vec())
            .await
            .unwrap();
        assert_eq!(ok.text, "# عنوان\n\nنص");
        assert_eq!(ok.warning, None);
    }

    #[tokio::test]
    async fn corrupt_pdf_is_an_error() {
        let err = extract("bad.pdf", b"%PDF-1.4 not really".to_vec())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("PDF"), "{err:#}");
    }

    #[test]
    fn arabic_pdf_sample_from_e2e() {
        // The shape of pdf-extract's output reported by the E2E test (Farsi-yeh
        // forms, visual order, U+FFFD).
        let raw = "ﻦﯿﻔظﻮﻤﻟا ﺔﺳﺎﯿﺳ \u{FFFD}\nﻲﻗاﺮﻋ رﺎﻨﯾد 75,000 هرﺪﻗ";
        let (text, warn) = fix_arabic_pdf(raw);
        assert_eq!(text, "سياسة الموظفين\nقدره 75,000 دينار عراقي");
        assert_eq!(warn, Some(WARN_ARABIC_PDF));
    }

    #[test]
    fn arabic_pdf_from_edge_print() {
        // Lines exactly as pdf-extract returns them for a headless-Edge Arabic PDF.
        let raw = ".ﺐﺗا ﺮﻟا ﻰﻟإ فﺎﻀ ُﻳ ﻲﻗا ﺮﻋ رﺎﻨﻳد  75,000  هرﺪﻗ ي ﺮﻬﺷ\n\
                   ﻲﻧوﺮﺘﻜﻟإلا ﺪﻳ ﺮﺒﻟا .ﺲﻴﻤﺨﻟا ﻰﻟإ ﺪﺣألا ﻦﻣ ا ًرصﻋ 4:00  ﺔﻋﺎﺴﻟا ﻲﻓ ﻲﻬﺘﻨﻳ و ﺎ ًﺣﺎﺒﺻ  8:00  ﺔﻋﺎﺴﻟا ﻲﻓ ماوﺪﻟا أﺪﺒﻳ\n\
                   . hr@example.com  ﺔﻳرشﺒﻟا د راﻮﻤﻠﻟ";
        let (text, warn) = fix_arabic_pdf(raw);
        assert_eq!(warn, Some(WARN_ARABIC_PDF));
        let lines: Vec<&str> = text.lines().collect();
        // Splits after ر/ز/و stay ambiguous ("عر اقي"); shaped or one-letter
        // fragments are glued back ("يُضاف", "شهري", "وينتهي", "للموارد").
        assert_eq!(lines[0], "شهري قدره 75,000 دينار عر اقي يُضاف إلى الر اتب.");
        assert_eq!(
            lines[1],
            "يبدأ الدوام في الساعة 8:00 صباحًا وينتهي في الساعة 4:00 عصرًا من الأحد إلى الخميس. البر يد الإلكتروني"
        );
        assert_eq!(lines[2], "للموارد البشرية hr@example.com .");
    }

    #[test]
    fn synthetic_visual_line_keeps_ltr_runs() {
        let logical = "استخدم Microsoft Word (النسخة الجديدة) الآن";
        // The same line stored left-to-right with shaped (isolated) glyphs.
        let visual = "ﻥﻵﺍ (ﺓﺪﻳﺪﺠﻟﺍ ﺔﺨﺴﻨﻟﺍ) Microsoft Word ﻡﺪﺨﺘﺳﺍ";
        let (text, warn) = fix_arabic_pdf(visual);
        assert_eq!(text, logical);
        assert_eq!(warn, Some(WARN_ARABIC_PDF));
        // Logical Arabic (a well-tagged PDF) and Latin text are left alone.
        assert_eq!(fix_arabic_pdf(logical), (logical.to_string(), None));
        assert_eq!(
            fix_arabic_pdf("Plain English 123\n"),
            ("Plain English 123\n".to_string(), None)
        );
    }

    #[test]
    fn pdf_blank_line_per_line_becomes_paragraphs() {
        let text = "Staff policy\n\nSalaries\n\nThe salary is paid on the 25th. Each employee gets a\n\n\
                    monthly transport allowance of 75,000 IQD.\n\nLeave\n\nTwenty-one days per year.";
        assert_eq!(
            pdf_paragraphs(text),
            "Staff policy\n\nSalaries\nThe salary is paid on the 25th. Each employee gets a\n\
             monthly transport allowance of 75,000 IQD.\n\nLeave\nTwenty-one days per year."
        );
        // Real paragraphs (adjacent lines exist) are left alone.
        let normal = "Line one\nline two\n\nNext paragraph";
        assert_eq!(pdf_paragraphs(normal), normal);
    }

    #[test]
    fn presentation_form_shapes() {
        assert_eq!(form('ﺷ'), Some(Form::Initial)); // FEB7
        assert_eq!(form('ﻀ'), Some(Form::Medial)); // FEC0
        assert_eq!(form('ﺮ'), Some(Form::Final)); // FEAE
        assert_eq!(form('ﺍ'), Some(Form::Isolated)); // FE8D
        assert_eq!(form('ﯿ'), Some(Form::Medial)); // FBFF
        assert_eq!(form('ر'), None);
        assert_eq!(unshape("ﻻ ﯿ"), "لا ي");
    }
}

#[cfg(test)]
mod live {
    /// Print the raw and repaired text of a PDF:
    /// `LAC_PDF=C:\path\file.pdf cargo test pdf_file_live -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn pdf_file_live() {
        let path = std::env::var("LAC_PDF").expect("set LAC_PDF");
        let bytes = std::fs::read(&path).unwrap();
        let raw = pdf_extract::extract_text_from_mem(&bytes).unwrap();
        let out = super::extract("x.pdf", bytes).await.unwrap();
        println!(
            "--- raw ---\n{raw}\n--- fixed ({:?}) ---\n{}",
            out.warning, out.text
        );
    }
}
