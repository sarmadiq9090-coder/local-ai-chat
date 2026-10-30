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

/// Extracted text plus an optional quality warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub text: String,
    pub warning: Option<&'static str>,
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
        let raw = tokio::task::spawn_blocking(move || pdf_extract::extract_text_from_mem(&bytes))
            .await
            .context("PDF extraction failed")?
            .map_err(|e| anyhow::anyhow!("could not read PDF: {e}"))?;
        let (text, warning) = fix_arabic_pdf(&raw);
        Ok(Extracted {
            text: pdf_paragraphs(&normalize(&text)),
            warning,
        })
    } else if TEXT_EXTS.contains(&ext.as_str()) {
        Ok(Extracted {
            text: normalize(&String::from_utf8_lossy(&bytes)),
            warning: None,
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

/// Rebuild one visual-order line in logical order.
fn fix_visual_line(line: &str) -> String {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let dirs: Vec<Dir> = tokens
        .iter()
        .map(|t| {
            if t.chars().any(is_arabic) {
                Dir::Rtl
            } else if t.chars().any(is_ltr) {
                Dir::Ltr
            } else {
                Dir::Neutral
            }
        })
        .collect();
    // Neutrals between two LTR tokens ("8:00 - 4:00") stay in the LTR run.
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
    // Consecutive LTR tokens form one run kept in its own order; each RTL
    // token is its own group. The groups are then reversed.
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
    groups.reverse();
    let words: Vec<Word> = groups.into_iter().flat_map(|(_, w)| w).collect();

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
