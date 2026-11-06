#!/usr/bin/env python3
"""Writes THIRD-PARTY-NOTICES.txt: the licence of every crate compiled into
local-ai-chat.exe (x86_64-pc-windows-msvc, normal dependencies only; build and
dev dependencies are not shipped), with the licence files each crate ships,
plus the bundled web assets (highlight.js).

    python3 tools/make_notices.py           # rewrite the file
    python3 tools/make_notices.py --check   # fail if it is out of date

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
# Web assets embedded from ui/vendor: (name, version, licence, licence file).
WEB_ASSETS = [
    ("highlight.js", "11.9.0", "BSD-3-Clause", "ui/vendor/LICENSE-highlightjs.txt"),
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


def render():
    packages = {}
    for target in TARGETS:
        meta = metadata(target)
        by_id = {p["id"]: p for p in meta["packages"]}
        for pid in shipped(meta):
            p = by_id[pid]
            packages[(p["name"], p["version"])] = p

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
        "Bundled web assets:",
        "",
    ]
    for name, version, lic, _ in WEB_ASSETS:
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

    for name, version, lic, path in WEB_ASSETS:
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


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="fail if THIRD-PARTY-NOTICES.txt is out of date")
    args = ap.parse_args()
    text = render()
    if args.check:
        try:
            with open(OUT, encoding="utf-8") as f:  # universal newlines: a CRLF checkout still matches
                current = f.read()
        except FileNotFoundError:
            current = ""
        if current != text:
            sys.exit("THIRD-PARTY-NOTICES.txt is out of date: run python3 tools/make_notices.py and commit it")
        print("THIRD-PARTY-NOTICES.txt is current")
        return
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    print(f"wrote {OUT} ({len(text) // 1024} KB)")


if __name__ == "__main__":
    main()
