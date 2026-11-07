#!/bin/sh
# TEST-ONLY minisign fixtures for the update-signature unit tests in src/update.rs.
#
# These keys are PUBLIC on purpose and protect nothing: they are used only under
# #[cfg(test)] (and by the debug-only `dev-test-update-key` feature for local
# end-to-end runs). A unit test asserts that none of them is ever in the
# production key list `update::TRUSTED_PUBLIC_KEYS`. NEVER use them to sign a release.
#
# Regenerate (needs `minisign`, e.g. `apt install minisign`), from this folder:
#   sh regen.sh
set -eu
cd "$(dirname "$0")"
rm -f ./*.pub ./*.key ./*.minisig manifest.json
for k in test-primary test-backup test-other; do
  minisign -G -W -f -p "$k.pub" -s "$k.key" >/dev/null
done
printf 'Lemon update-signature test payload (not a real exe).\n' > payload.bin
sign() { # <key> <sig file> <trusted comment> [extra minisign flags]
  key=$1; out=$2; tc=$3; shift 3
  minisign -S "$@" -s "$key.key" -m payload.bin -x "$out" -t "$tc" >/dev/null
}
sign test-primary payload.bin.minisig               "local-ai-chat v9.9.9"
sign test-backup  payload.bin.backup.minisig        "local-ai-chat v9.9.9"
sign test-other   payload.bin.other-key.minisig     "local-ai-chat v9.9.9"
sign test-primary payload.bin.wrong-version.minisig "local-ai-chat v9.9.8"
sign test-primary payload.bin.wrong-name.minisig    "lemon v9.9.9"
sign test-primary payload.bin.legacy.minisig        "local-ai-chat v9.9.9" -l
# The update manifest (docs/specs/update-manifest.md) for a fake v9.9.9 whose
# exe is payload.bin, made by the real tool and signed the same way.
tmp=$(mktemp -d)
cp payload.bin "$tmp/local-ai-chat.exe"
python3 ../../../tools/make-update-manifest build --app local-ai-chat --version 9.9.9 \
  --repo sarmadiq9090-coder/local-ai-chat --released 2026-11-13T06:00:00Z \
  --notes-url https://fastkiwi.app/lemon/ --asset x64="$tmp/local-ai-chat.exe" \
  --out manifest.json >/dev/null
rm -r "$tmp"
msign() { # <key> <sig file> <trusted comment> [extra minisign flags]
  key=$1; out=$2; tc=$3; shift 3
  minisign -S "$@" -s "$key.key" -m manifest.json -x "$out" -t "$tc" >/dev/null
}
msign test-primary manifest.json.minisig               "local-ai-chat update-manifest v9.9.9"
msign test-backup  manifest.json.backup.minisig        "local-ai-chat update-manifest v9.9.9"
msign test-other   manifest.json.other-key.minisig     "local-ai-chat update-manifest v9.9.9"
msign test-primary manifest.json.wrong-version.minisig "local-ai-chat update-manifest v9.9.8"
msign test-primary manifest.json.exe-comment.minisig   "local-ai-chat v9.9.9"
msign test-primary manifest.json.legacy.minisig        "local-ai-chat update-manifest v9.9.9" -l
echo "fixtures regenerated"
