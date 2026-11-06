//! ar/en string parity for the UI: every `ui/core/strings.js` and
//! `ui/features/*/strings.js` must define the same keys in both languages,
//! with no empty values and the same `{placeholders}`.
//!
//! The files are plain object literals (`export default { ar: {...}, en: {...} }`
//! plus optional `export const NAME = { ar: ..., en: ... }`), so a tiny parser
//! for JS literals (strings, `+` concatenation, arrays, objects, numbers,
//! booleans) is enough; no JS engine needed. Runs on every `cargo test`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const LANGS: [&str; 2] = ["ar", "en"];

#[derive(Debug)]
enum Val {
    Str(String),
    Arr(Vec<Val>),
    Obj(BTreeMap<String, Val>),
    Other,
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    file: &'a str,
}

impl<'a> Parser<'a> {
    fn err(&self, msg: &str) -> String {
        let line = self.s[..self.i.min(self.s.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            + 1;
        format!("{}:{line}: {msg}", self.file)
    }

    /// Skip whitespace and comments.
    fn ws(&mut self) {
        loop {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            if self.s[self.i..].starts_with(b"//") {
                while self.i < self.s.len() && self.s[self.i] != b'\n' {
                    self.i += 1;
                }
            } else if self.s[self.i..].starts_with(b"/*") {
                match find(&self.s[self.i + 2..], b"*/") {
                    Some(p) => self.i += 2 + p + 2,
                    None => self.i = self.s.len(),
                }
            } else {
                return;
            }
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected '{}'", c as char)))
        }
    }

    fn ident(&mut self) -> Option<String> {
        self.ws();
        let start = self.i;
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_alphanumeric() || matches!(self.s[self.i], b'_' | b'$'))
        {
            self.i += 1;
        }
        (self.i > start).then(|| String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }

    fn string(&mut self) -> Result<String, String> {
        let q = self.s[self.i];
        self.i += 1;
        let mut out = Vec::new();
        while self.i < self.s.len() {
            let c = self.s[self.i];
            self.i += 1;
            if c == q {
                return String::from_utf8(out).map_err(|_| self.err("invalid UTF-8"));
            }
            if q == b'`' && c == b'$' && self.s.get(self.i) == Some(&b'{') {
                return Err(self.err("template interpolation is not allowed in strings.js"));
            }
            if c == b'\\' {
                let e = self.s[self.i];
                self.i += 1;
                match e {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'u' => {
                        // \uXXXX or \u{...}: keep a placeholder char; content checks don't need it.
                        if self.s.get(self.i) == Some(&b'{') {
                            while self.i < self.s.len() && self.s[self.i] != b'}' {
                                self.i += 1;
                            }
                            self.i += 1;
                        } else {
                            self.i += 4;
                        }
                        out.push(b'?');
                    }
                    b'\n' => {}
                    other => out.push(other),
                }
            } else {
                out.push(c);
            }
        }
        Err(self.err("unterminated string"))
    }

    fn value(&mut self) -> Result<Val, String> {
        let v = match self.peek() {
            Some(b'"' | b'\'' | b'`') => {
                let mut s = self.string()?;
                // "a" + "b" + ...
                while self.peek() == Some(b'+') {
                    self.i += 1;
                    match self.peek() {
                        Some(b'"' | b'\'' | b'`') => s.push_str(&self.string()?),
                        _ => return Err(self.err("only string literals may be concatenated")),
                    }
                }
                Val::Str(s)
            }
            Some(b'{') => self.object()?,
            Some(b'[') => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    if self.peek() == Some(b']') {
                        self.i += 1;
                        break;
                    }
                    items.push(self.value()?);
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b']') => {}
                        _ => return Err(self.err("expected ',' or ']'")),
                    }
                }
                Val::Arr(items)
            }
            Some(c) if c.is_ascii_digit() || c == b'-' => {
                while self.i < self.s.len()
                    && (self.s[self.i].is_ascii_alphanumeric()
                        || matches!(self.s[self.i], b'.' | b'-'))
                {
                    self.i += 1;
                }
                Val::Other
            }
            _ => match self.ident().as_deref() {
                Some("true" | "false" | "null") => Val::Other,
                _ => {
                    return Err(self.err("unsupported value (strings.js must hold plain literals)"));
                }
            },
        };
        Ok(v)
    }

    fn object(&mut self) -> Result<Val, String> {
        self.expect(b'{')?;
        let mut map = BTreeMap::new();
        loop {
            if self.peek() == Some(b'}') {
                self.i += 1;
                break;
            }
            let key = match self.peek() {
                Some(b'"' | b'\'') => self.string()?,
                _ => self.ident().ok_or_else(|| self.err("expected a key"))?,
            };
            self.expect(b':')?;
            let v = self.value()?;
            if map.insert(key.clone(), v).is_some() {
                return Err(self.err(&format!("duplicate key '{key}'")));
            }
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {}
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
        Ok(Val::Obj(map))
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Every `export default {` / `export const NAME = {` object in the file, by export name.
fn exports(file: &str, src: &str) -> Result<Vec<(String, Val)>, String> {
    let mut out = Vec::new();
    let mut p = Parser {
        s: src.as_bytes(),
        i: 0,
        file,
    };
    while let Some(off) = find(&p.s[p.i..], b"export ") {
        p.i += off + "export ".len();
        let name = match p.ident().as_deref() {
            Some("default") => "default".to_string(),
            Some("const") => {
                let n = p.ident().ok_or_else(|| p.err("expected a name"))?;
                p.expect(b'=')?;
                n
            }
            _ => continue,
        };
        if p.peek() == Some(b'{') {
            out.push((name, p.object()?));
        }
    }
    Ok(out)
}

/// `{name}` placeholders in a string.
fn placeholders(s: &str) -> BTreeSet<&str> {
    let mut set = BTreeSet::new();
    let mut rest = s;
    while let Some(a) = rest.find('{') {
        rest = &rest[a + 1..];
        match rest.find('}') {
            Some(b) if rest[..b].chars().all(|c| c.is_alphanumeric() || c == '_') && b > 0 => {
                set.insert(&rest[..b]);
                rest = &rest[b + 1..];
            }
            _ => {}
        }
    }
    set
}

/// `("asst_chunks", "two")` for a plural-form key like `asst_chunks_two`.
fn plural_base(key: &str) -> Option<(&str, &str)> {
    let (base, form) = key.rsplit_once('_')?;
    matches!(form, "zero" | "one" | "two" | "few" | "many" | "other").then_some((base, form))
}

/// Compare the `ar` value (reference) with another language's value at `path`.
fn compare(path: &str, lang: &str, a: &Val, b: &Val, errs: &mut Vec<String>) {
    compare_inner(path, lang, a, b, true, errs);
}

fn compare_inner(
    path: &str,
    lang: &str,
    a: &Val,
    b: &Val,
    check_vars: bool,
    errs: &mut Vec<String>,
) {
    match (a, b) {
        (Val::Str(x), Val::Str(y)) => {
            for (l, s) in [("ar", x), (lang, y)] {
                if s.trim().is_empty() {
                    errs.push(format!("{path}: empty value in '{l}'"));
                }
            }
            let (px, py) = (placeholders(x), placeholders(y));
            if check_vars && px != py {
                errs.push(format!(
                    "{path}: placeholders differ: ar {px:?} vs {lang} {py:?}"
                ));
            }
        }
        (Val::Arr(x), Val::Arr(y)) => {
            if x.len() != y.len() {
                errs.push(format!(
                    "{path}: ar has {} items, {lang} has {}",
                    x.len(),
                    y.len()
                ));
            }
            for (i, (u, v)) in x.iter().zip(y).enumerate() {
                compare(&format!("{path}[{i}]"), lang, u, v, errs);
            }
        }
        (Val::Obj(x), Val::Obj(y)) => {
            // t() falls back to Arabic, so a missing key shows Arabic text in English.
            // Exception: plural forms English never selects (key_two/_few/_many when
            // key_other exists); assistants.js plural() picks key_<Intl rule>.
            for k in x.keys().filter(|k| !y.contains_key(*k)) {
                let unused_form = plural_base(k).is_some_and(|(b, f)| {
                    matches!(f, "two" | "few" | "many") && y.contains_key(&format!("{b}_other"))
                });
                if !(lang == "en" && unused_form) {
                    errs.push(format!("{path}.{k}: missing in '{lang}'"));
                }
            }
            for k in y.keys().filter(|k| !x.contains_key(*k)) {
                errs.push(format!("{path}.{k}: missing in 'ar'"));
            }
            for (k, u) in x {
                if let Some(v) = y.get(k) {
                    // Plural forms may legitimately drop {n} ("مقطع واحد" vs "{n} chunk").
                    let is_form =
                        plural_base(k).is_some_and(|(b, _)| x.contains_key(&format!("{b}_other")));
                    compare_inner(&format!("{path}.{k}"), lang, u, v, !is_form, errs);
                }
            }
        }
        (Val::Other, Val::Other) => {}
        _ => errs.push(format!("{path}: value types differ between ar and {lang}")),
    }
}

fn strings_files(root: &Path) -> Vec<PathBuf> {
    let mut files = vec![root.join("ui/core/strings.js")];
    let mut features: Vec<_> = fs::read_dir(root.join("ui/features"))
        .expect("read ui/features")
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("strings.js"))
        .filter(|p| p.is_file())
        .collect();
    features.sort();
    files.extend(features);
    files
}

fn check_file(root: &Path, path: &Path, errs: &mut Vec<String>) -> usize {
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let src = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {rel}: {e}"));
    let exported = match exports(&rel, &src) {
        Ok(x) => x,
        Err(e) => {
            errs.push(e);
            return 0;
        }
    };
    let mut checked = 0;
    for (name, val) in exported {
        let Val::Obj(map) = &val else { continue };
        // Only objects keyed by language take part (default must be one).
        if name != "default" && !map.contains_key("ar") {
            continue;
        }
        let prefix = format!("{rel} ({name})");
        for l in LANGS {
            if !map.contains_key(l) {
                errs.push(format!("{prefix}: no '{l}' section"));
            }
        }
        for k in map.keys().filter(|k| !LANGS.contains(&k.as_str())) {
            errs.push(format!(
                "{prefix}: unexpected top-level key '{k}' (only ar/en)"
            ));
        }
        if let (Some(ar), Some(en)) = (map.get("ar"), map.get("en")) {
            compare(&prefix, "en", ar, en, errs);
            checked += 1;
        }
    }
    if checked == 0 {
        errs.push(format!("{rel}: no `export default {{ ar, en }}` found"));
    }
    checked
}

#[test]
fn ui_strings_have_ar_en_parity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = strings_files(root);
    assert!(files.len() > 1, "no feature strings.js found");
    let mut errs = Vec::new();
    for f in &files {
        check_file(root, f, &mut errs);
    }
    assert!(
        errs.is_empty(),
        "ar/en string parity problems:\n  {}",
        errs.join("\n  ")
    );
}

#[test]
fn parity_check_catches_gaps() {
    let src = r#"// sample
export default {
  ar: { a: "أ", b: "مرحبا {name}", c: "", list: ["x", "y"],
        n_zero: "لا شيء", n_one: "واحد", n_two: "اثنان", n_other: "{n}" },
  en: { a: "A", b: "Hello", list: ["x"], d: 'only' + " en", n_one: "{n} thing", n_other: "{n} things" },
};"#;
    let ex = exports("t.js", src).unwrap();
    let Val::Obj(m) = &ex[0].1 else { panic!() };
    let mut errs = Vec::new();
    compare("t", "en", &m["ar"], &m["en"], &mut errs);
    let all = errs.join("\n");
    assert!(all.contains("t.n_zero: missing in 'en'"), "{all}");
    assert!(!all.contains("n_two") && !all.contains("t.n_one"), "{all}");
    for want in [
        "t.c: missing in 'en'",
        "t.d: missing in 'ar'",
        "t.b: placeholders differ",
        "t.list: ar has 2 items",
    ] {
        assert!(all.contains(want), "expected {want:?} in:\n{all}");
    }
}
