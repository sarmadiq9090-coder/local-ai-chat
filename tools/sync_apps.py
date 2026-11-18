#!/usr/bin/env python3
"""Refresh the vendored "More from FastKiwi" data from the site repo.

Copies `data/apps.json` and the icons it names (every app except Lemon itself)
from a checkout of the FastKiwi site repo (sarmadiq9090-coder.github.io) at a
given git ref into `ui/features/morefrom/data/`, and records the source commit
in `ui/features/morefrom/data/SOURCE.txt`. The app never fetches this list at
runtime: it is embedded in the exe (see src/more_from.rs).

Usage (standard library only):
    python3 tools/sync_apps.py --site ../sarmadiq9090-coder.github.io [--ref main]
Then run `cargo test` and commit the changed files under ui/features/morefrom/data/.
"""

import argparse
import json
import pathlib
import re
import subprocess
import sys

SELF_ID = "lemon"  # must match more_from::SELF_ID in src/more_from.rs
ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "ui" / "features" / "morefrom" / "data"
ICON_RE = re.compile(r"^img/[a-z0-9-]+\.(svg|png)$")


def git(site, *args):
    return subprocess.run(["git", "-C", str(site), *args], check=True, capture_output=True).stdout


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--site", required=True, help="path to a checkout of the site repo")
    ap.add_argument("--ref", default="main", help="git ref to copy from (default: main)")
    args = ap.parse_args()

    commit = git(args.site, "rev-parse", "--verify", f"{args.ref}^{{commit}}").decode().strip()
    raw = git(args.site, "show", f"{commit}:data/apps.json")
    data = json.loads(raw)
    if data.get("schema") != 1:
        sys.exit(f"unsupported apps.json schema {data.get('schema')!r}: update src/more_from.rs first")

    icons = set()
    for app in data["apps"]:
        if app["id"] == SELF_ID:
            continue
        if not ICON_RE.match(app["icon"]):
            sys.exit(f"unexpected icon path for {app['id']}: {app['icon']!r}")
        icons.add(app["icon"])

    (OUT / "img").mkdir(parents=True, exist_ok=True)
    (OUT / "apps.json").write_bytes(raw)
    for old in (OUT / "img").iterdir():
        if f"img/{old.name}" not in icons:
            old.unlink()
    for icon in sorted(icons):
        (OUT / icon).write_bytes(git(args.site, "show", f"{commit}:{icon}"))
    (OUT / "SOURCE.txt").write_text(
        "Vendored from the FastKiwi site repo (sarmadiq9090-coder/sarmadiq9090-coder.github.io):\n"
        f"commit {commit}\n"
        "files: data/apps.json and the icons in img/ (every app except Lemon).\n"
        "Refresh: python3 tools/sync_apps.py --site <site checkout> --ref <ref>\n",
        encoding="utf-8",
        newline="\n",
    )
    print(f"synced apps.json and {len(icons)} icons from {commit[:12]}")


if __name__ == "__main__":
    main()
