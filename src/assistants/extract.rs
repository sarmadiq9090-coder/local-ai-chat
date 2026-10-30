//! Plain-text extraction for knowledge documents.
//!
//! Text-like files are decoded as UTF-8 (lossy); PDFs go through the
//! `pdf-extract` crate on a blocking thread. Everything else is rejected with
//! "unsupported file type". The result is normalized (no NULs, `\n` line
//! endings, collapsed runs of spaces and blank lines).

use anyhow::{Context, bail};

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
pub async fn extract(name: &str, bytes: Vec<u8>) -> anyhow::Result<String> {
    let ext = extension(name).unwrap_or_default();
    let raw = if ext == "pdf" {
        tokio::task::spawn_blocking(move || pdf_extract::extract_text_from_mem(&bytes))
            .await
            .context("PDF extraction failed")?
            .map_err(|e| anyhow::anyhow!("could not read PDF: {e}"))?
    } else if TEXT_EXTS.contains(&ext.as_str()) {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        bail!("unsupported file type: {name}");
    };
    Ok(normalize(&raw))
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
        assert_eq!(ok, "# عنوان\n\nنص");
    }

    #[tokio::test]
    async fn corrupt_pdf_is_an_error() {
        let err = extract("bad.pdf", b"%PDF-1.4 not really".to_vec())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("PDF"), "{err:#}");
    }
}
