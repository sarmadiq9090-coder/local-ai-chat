#!/usr/bin/env python3
"""Writes THIRD-PARTY-NOTICES.txt: the licence of every crate compiled into
local-ai-chat.exe (x86_64-pc-windows-msvc, normal dependencies only; build and
dev dependencies are not shipped), with the licence files each crate ships,
plus the bundled web assets in ui/vendor (highlight.js). It also rewrites the
generated tables in THIRD_PARTY_NOTICES.md (between the "generated:" markers),
so neither file is edited by hand.

    python3 tools/make_notices.py           # rewrite both files
    python3 tools/make_notices.py --check   # fail if either is out of date

Inputs: `cargo metadata` (Cargo.lock and the crate sources it downloads) and
ui/vendor. The version and licence of each web asset are read from the file's
own banner, and every file in ui/vendor must be listed in WEB_ASSETS, so a
vendor update or a new vendor file cannot slip past the check.

Every crate's licence must be satisfiable with ALLOWED (or be listed in
EXCEPTIONS, which name the crate and the extra licence); otherwise the script
fails, in both modes. The exe embeds the file (GET /api/app/notices, shown in
Settings -> Support), so every release ships it. Rerun after any Cargo.lock
change; CI (ci.yml, job "notices") runs --check.
"""
import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "THIRD-PARTY-NOTICES.txt")
SUMMARY = os.path.join(ROOT, "THIRD_PARTY_NOTICES.md")
VENDOR = "ui/vendor"
TARGETS = ["x86_64-pc-windows-msvc"]
LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice|copyright|unlicense)([-_.].*)?$", re.I)

# Permissive licences (the owner's allowlist used across the FastKiwi repos).
ALLOWED = {
    "MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause",
    "Unicode-3.0", "ISC", "Zlib", "BSL-1.0", "0BSD", "CC0-1.0", "Unlicense",
}
# crate name -> licences allowed for that crate only.
EXCEPTIONS = {
    # A helper of `dirs`, used unmodified; source on crates.io (see THIRD_PARTY_NOTICES.md).
    "option-ext": {"MPL-2.0"},
}
# Web assets embedded from ui/vendor: (name, home page, file, licence file,
# banner regex). The version and the SPDX licence come from the file's banner.
WEB_ASSETS = [
    ("highlight.js", "https://highlightjs.org/", "ui/vendor/highlight.min.js",
     "ui/vendor/LICENSE-highlightjs.txt", r"Highlight\.js v(?P<version>\S+)"),
]


def metadata(target):
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target],
        cwd=ROOT, check=True, capture_output=True, text=True, encoding="utf-8",
    ).stdout
    return json.loads(out)


def shipped(meta):
    """Package ids reachable from the root through normal dependencies."""
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]
    seen, todo = set(), [root]
    while todo:
        cur = todo.pop()
        for dep in nodes[cur]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                todo.append(dep["pkg"])
    return seen


def read_text(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read().strip().replace("\r\n", "\n")


def license_texts(pkg):
    folder = os.path.dirname(pkg["manifest_path"])
    texts = []
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if LICENSE_FILE.match(name) and os.path.isfile(path):
            texts.append((name, read_text(path)))
    if pkg.get("license_file"):
        path = os.path.join(folder, pkg["license_file"])
        if os.path.isfile(path) and not any(n == os.path.basename(path) for n, _ in texts):
            texts.append((os.path.basename(path), read_text(path)))
    return texts


def web_assets():
    """(name, home, version, licence, file, licence file) for each WEB_ASSETS entry,
    read from the files themselves. Fails on a vendor file nobody listed."""
    listed = set()
    out = []
    for name, home, path, lic_path, banner in WEB_ASSETS:
        listed.update((path, lic_path))
        with open(os.path.join(ROOT, path), encoding="utf-8", errors="replace") as f:
            head = f.read(2000)
        version = re.search(banner, head)
        lic = re.search(r"License:\s*([A-Za-z0-9.+-]+)", head)
        if not version or not lic:
            sys.exit(f"{path}: no version/licence banner (WEB_ASSETS in tools/make_notices.py)")
        if not os.path.isfile(os.path.join(ROOT, lic_path)):
            sys.exit(f"{lic_path}: licence file missing for {name}")
        out.append((name, home, version.group("version"), lic.group(1), path, lic_path))
    present = {f"{VENDOR}/{n}" for n in os.listdir(os.path.join(ROOT, VENDOR))}
    unlisted = sorted(present - listed)
    if unlisted:
        sys.exit("Files in ui/vendor that WEB_ASSETS in tools/make_notices.py does not list "
                 "(add them with their licence file):\n  " + "\n  ".join(unlisted))
    return out


def satisfiable(expr, allowed):
    """Can the SPDX expression `expr` be met using only `allowed` licences?
    Handles OR, AND, WITH, parentheses and the legacy "MIT/Apache-2.0" form."""
    tokens = re.findall(r"\(|\)|[^\s()]+", expr.replace("/", " OR "))
    pos = 0

    def peek():
        return tokens[pos] if pos < len(tokens) else None

    def atom():
        nonlocal pos
        tok = peek()
        if tok is None:
            raise ValueError(f"bad licence expression: {expr!r}")
        pos += 1
        if tok == "(":
            value = either()
            if peek() != ")":
                raise ValueError(f"bad licence expression: {expr!r}")
            pos += 1
            return value
        if peek() == "WITH":
            pos += 1
            exc = peek()
            if exc is None or exc in ("(", ")", "AND", "OR", "WITH"):
                raise ValueError(f"bad licence expression: {expr!r}")
            pos += 1
            return f"{tok} WITH {exc}" in allowed
        return tok in allowed

    def both():
        nonlocal pos
        value = atom()
        while peek() == "AND":
            pos += 1
            value = atom() and value
        return value

    def either():
        nonlocal pos
        value = both()
        while peek() == "OR":
            pos += 1
            value = both() or value
        return value

    result = either()
    if pos != len(tokens):
        raise ValueError(f"bad licence expression: {expr!r}")
    return result


def collect():
    """(shipped packages by (name, version), direct dependencies, metadata of TARGETS[0])."""
    packages, first = {}, None
    for target in TARGETS:
        meta = metadata(target)
        first = first or meta
        by_id = {p["id"]: p for p in meta["packages"]}
        for pid in shipped(meta):
            p = by_id[pid]
            packages[(p["name"], p["version"])] = p
    return packages, direct_deps(first)


def direct_deps(meta):
    """[(name, version, licence, note)] for the root's normal and build dependencies."""
    by_id = {p["id"]: p for p in meta["packages"]}
    root = by_id[meta["resolve"]["root"]]
    node = next(n for n in meta["resolve"]["nodes"] if n["id"] == root["id"])
    resolved = {by_id[d["pkg"]]["name"]: by_id[d["pkg"]] for d in node["deps"]}
    rows = {}
    for dep in root["dependencies"]:
        if dep["kind"] == "dev" or dep["name"] not in resolved:
            continue
        p = resolved[dep["name"]]
        note = ""
        if dep["kind"] == "build":
            note = "build-time only, not shipped"
        elif dep.get("target"):
            note = "Windows only" if "windows" in dep["target"] else dep["target"]
        rows[p["name"]] = (p["name"], p["version"], p.get("license") or "see licence file", note)
    return [rows[k] for k in sorted(rows)]


def render(packages, assets):
    refused = []
    for (name, version), p in sorted(packages.items()):
        expr = p.get("license")
        if not expr or not satisfiable(expr, ALLOWED | EXCEPTIONS.get(name, set())):
            refused.append(f"  {name} {version}: {expr or 'no SPDX licence (license-file only)'}")
    if refused:
        sys.exit("These crates have a licence outside the allowlist (tools/make_notices.py ALLOWED/"
                 "EXCEPTIONS; adding one needs the owner's approval):\n" + "\n".join(refused))

    lines = [
        "Lemon (local-ai-chat.exe) - third-party notices",
        "",
        "Lemon is MIT licensed (LICENSE). It includes the following open-source",
        "software, under the licences below. Generated by tools/make_notices.py",
        "from Cargo.lock; do not edit by hand.",
        "",
        "Microsoft Edge WebView2: webview2-com-sys links Microsoft's",
        "WebView2LoaderStatic.lib, distributed under the Microsoft WebView2 SDK licence.",
        "",
        "Fonts and icons: no font files are bundled (the UI uses fonts installed on the",
        "user's Windows); the UI icons are inline SVG in ui/core/icons.js and the app",
        "icon is generated by assets/source/make_lemon_icons.py, both under this project's",
        "MIT licence.",
        "",
        "Not bundled: Ollama and the AI models are separate software the user installs,",
        "each under its own licence (Ollama: MIT; every model has its own licence).",
        "Microsoft Edge WebView2 Runtime is likewise a Windows component, not redistributed.",
        "",
        "Bundled web assets:",
        "",
    ]
    for name, _, version, lic, _, _ in assets:
        lines.append(f"  {name} {version} - {lic}")
    lines += ["", f"Rust crates ({len(packages)}):", ""]
    for (name, version), p in sorted(packages.items()):
        lines.append(f"  {name} {version} - {p.get('license') or 'see licence file'}")

    # Each distinct licence text once (most crates share the Apache and MIT
    # texts word for word), referred to by number from each component.
    texts, numbers = [], {}

    def ref(text):
        key = re.sub(r"\s+", " ", text)
        if key not in numbers:
            texts.append(text)
            numbers[key] = len(texts)
        return numbers[key]

    for name, _, version, lic, _, path in assets:
        lines += ["", "=" * 78, f"{name} {version}", f"Licence: {lic}"]
        lines.append(f"{os.path.basename(path)}: text [{ref(read_text(os.path.join(ROOT, path)))}] below")
    for (name, version), p in sorted(packages.items()):
        lines += ["", "=" * 78, f"{name} {version}", f"Licence: {p.get('license') or 'see below'}"]
        if p.get("repository"):
            lines.append(f"Source: {p['repository']}")
        files = license_texts(p)
        if not files:
            lines.append("(The crate ships no licence file; the licence named above applies.)")
        for fname, text in files:
            lines.append(f"{fname}: text [{ref(text)}] below")
    for i, text in enumerate(texts, 1):
        lines += ["", "#" * 78, f"[{i}]", "", text]
    return "\n".join(lines) + "\n"


def normalise(expr):
    """One spelling per plain OR-list ("MIT/Apache-2.0" == "Apache-2.0 OR MIT")."""
    if re.search(r"\(|\)|\bAND\b|\bWITH\b", expr):
        return expr
    return " OR ".join(sorted(x.strip() for x in re.split(r"\s*/\s*|\s+OR\s+", expr)))


def md_blocks(packages, direct, assets):
    """Generated tables of THIRD_PARTY_NOTICES.md, by marker name."""
    web = ["| Component | Version | License | Files |", "|---|---|---|---|"]
    for name, home, version, lic, path, lic_path in assets:
        web.append(f"| [{name}]({home}) | {version} | {lic} | `{path}` — full license text in "
                   f"[`{lic_path}`]({lic_path}) |")
    crates = ["| Crate | Version | License |", "|---|---|---|"]
    for name, version, lic, note in direct:
        crates.append(f"| {name}{f' ({note})' if note else ''} | {version} | {lic} |")
    counts = {}
    for p in packages.values():
        key = normalise(p.get("license") or "see licence file")
        counts[key] = counts.get(key, 0) + 1
    groups = [f"The Windows build ships {len(packages)} crates. Their licenses, as declared in",
              "each crate's `Cargo.toml` (spellings of the same OR-list merged):", "",
              "| License expression | Crates |", "|---|---|"]
    for key, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])):
        groups.append(f"| {key} | {n} |")
    return {"web-assets": web, "direct-crates": crates, "licence-groups": groups}


def render_md(current, blocks):
    for name, rows in blocks.items():
        start, end = f"<!-- generated:{name} -->", f"<!-- /generated:{name} -->"
        pattern = re.compile(re.escape(start) + r".*?" + re.escape(end), re.S)
        if not pattern.search(current):
            sys.exit(f"THIRD_PARTY_NOTICES.md: markers {start} ... {end} not found")
        body = "\n".join([start, "<!-- Written by tools/make_notices.py; do not edit by hand. -->", *rows, end])
        current = pattern.sub(lambda _: body, current, count=1)
    return current


def read_or_empty(path):
    try:
        with open(path, encoding="utf-8") as f:  # universal newlines: a CRLF checkout still matches
            return f.read()
    except FileNotFoundError:
        return ""


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="fail if either file is out of date")
    args = ap.parse_args()
    assets = web_assets()
    packages, direct = collect()
    outputs = {
        OUT: render(packages, assets),
        SUMMARY: render_md(read_or_empty(SUMMARY), md_blocks(packages, direct, assets)),
    }
    if args.check:
        stale = [os.path.basename(p) for p, text in outputs.items() if read_or_empty(p) != text]
        if stale:
            sys.exit(f"{', '.join(stale)} out of date: run python3 tools/make_notices.py and commit the result")
        print("THIRD-PARTY-NOTICES.txt and THIRD_PARTY_NOTICES.md are current")
        return
    for path, text in outputs.items():
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print(f"wrote {path} ({len(text) // 1024} KB)")


if __name__ == "__main__":
    main()
