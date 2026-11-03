# Spec: Signed updates (minisign)

The auto-updater (`src/update.rs`, spec `docs/specs/auto-update.md`) replaces the
running exe with a file downloaded from the latest GitHub release. This spec makes
sure that file was released by the owner, not just by whoever can edit releases.

Owner steps in Arabic: [`RELEASING.md`](../../RELEASING.md).

## Threat model

**Problem.** Until 2.2.0 the updater trusted `local-ai-chat.exe` because it matched
`local-ai-chat.exe.sha256`, and both come from the same release. Anyone who can
edit releases can ship malware to every install: a stolen token or session, a
collaborator account, or a compromised third-party action in the release workflow.

**Defence.** Every release exe gets a minisign signature
(`local-ai-chat.exe.minisig`). The app trusts only the public keys compiled into it
(`update::TRUSTED_PUBLIC_KEYS`: a primary key and a backup key). The signature's
trusted comment must be exactly `local-ai-chat <tag>`, e.g.
`local-ai-chat v2.3.0`. The comment is covered by the signature, so an older
signed exe cannot be replayed as a newer version.

| Attacker can… | Result |
|---|---|
| upload or replace release assets (token, collaborator, compromised action in `publish`) | **Blocked.** No secret key, so no valid `.minisig`. The update is refused. |
| re-publish an older signed exe under a new tag | **Blocked.** The trusted comment names the old version. |
| push a `v*` tag, or a version bump to `main`, to start the release workflow | The `sign` job **waits for the owner's approval** in the `release` Environment. Approve only runs you started yourself. |
| delete the `.minisig`, or serve a broken one | **Blocked.** A missing or invalid signature means refuse, never fall back. |
| change the app's key list | Only through a signed release, so they would need the key first. |
| compromise the **build** (malicious dependency or action in `build`) | **Not covered by signing**: the owner approves, and CI signs whatever was built. Mitigated by SHA-pinned actions, `--locked`, no build cache, Dependabot review, and the provenance attestation for audit. |
| take over the owner's GitHub account **and** approve | **Not covered**: they can trigger a real signed release. Mitigated by 2FA, a tag ruleset, and the approval being visible (mobile notifications). |
| steal the backup key | Needs the owner's password manager. The key is not on GitHub or any PC. |

Downgrades are not offered: the updater only installs a strictly newer version.

**Transition.** The key list ships **empty**. Builds with an empty list behave
exactly like 2.2.0 (sha256 only) and log `signature checking is not configured`.
Once the owner has keys, a follow-up PR pastes them in. Every build after that
**requires** a valid signature. The switch is compile-time only: there is no
setting, env var or server flag that skips the check. Clients on 2.2.0 or older,
or on any build with an empty list, stay unprotected until they update to a build
with keys. That first update is itself checked by sha256 only.

## App side (`src/update.rs`)

- Crate `minisign-verify` (verify-only, no dependencies).
- `TRUSTED_PUBLIC_KEYS: &[&str]`: base64 public keys (line 2 of a `.pub` file),
  primary first. Marked `// TODO(owner): paste minisign public keys here`.
- `install()` → `download_verified(release, trusted_keys(), …)`. It downloads the
  sha256 and the exe, checks the sha256, and then, if there are keys:
  1. The release must list `local-ai-chat.exe.minisig`. Its URL follows the same
     https / loopback rules as the other assets and is capped at 4 KB.
  2. `verify_signature(exe, sig, version, keys)`. The signature must be
     pre-hashed (legacy is refused) and made by one of the keys. Its trusted comment
     must equal `expected_trusted_comment(version)` = `local-ai-chat v<version>`.
     `version` is the release tag normalized (`v2.3.0` → `2.3.0`). The release
     workflow only accepts tags that normalize to themselves.
  3. Any failure is a `SignatureError`. Nothing is written to disk. The install
     stream ends with `{"error": "...", "code": "signature"}`, and the UI
     (`ui/features/update`) shows "couldn't be verified" in ar and en, with links
     to the releases page and https://fastkiwi.app/.
- `LOCALAI_UPDATE_FEED` (dev feed) gets exactly the same checks.
- Tests (`cargo test update::`) use a committed **TEST-ONLY** key set in
  `tests/fixtures/update-signing/` (regenerate with `regen.sh`). They cover: valid,
  backup key, tampered exe, other key, wrong version or name in the comment,
  missing, malformed or legacy signature, and the production list never holding a
  test key. They also check that every production key parses.
- Debug-only feature `dev-test-update-key` trusts the test key instead, for local
  end-to-end runs against a fake feed. A release build with it does not compile
  (`compile_error!`).

## Release workflow (`.github/workflows/release.yml`)

Top-level `permissions: contents: read`. Every `uses:` is pinned to a commit SHA,
and `.github/dependabot.yml` bumps them weekly.

Triggers: a push to `main` (releases only when the `Cargo.toml` version has no
`v<version>` tag yet, so releasing = merging a version-bump PR), a manual `v*` tag
push (kept for compatibility), and `workflow_dispatch` (dry run). Concurrency group
`release`, never cancelled in progress. On `main` the tag is created by `publish`
(`gh release create --target <sha>`); a tag made with `GITHUB_TOKEN` does not
start another run.

| Job | Runs on | Token / secrets | Does |
|---|---|---|---|
| `version` | ubuntu | `contents: read`, no checkout | Reads `Cargo.toml` at the commit through the API and asks whether tag `v<version>` exists. Outputs `new=true` for a `main` push with no such tag, or for a tag push (which must equal `v<Cargo version>`). When releasing, checks the tag has no leading zeros and no `+build`. With `new=false` every other job is skipped. |
| `build` | windows | `contents: read`, no secrets, no cache, checkout with `persist-credentials: false` | Runs the tests and `--release --locked` build, writes `.sha256` and the notes (the tag message, or the `## [<version>]` section of `CHANGELOG.md` on `main`), and uploads artifact `release-dist`. |
| `sign` | ubuntu | Environment **`release`** (owner approval; branch `main` and `v*` tags only). `MINISIGN_SECRET_KEY`, `MINISIGN_PASSWORD`. `contents: read` | Runs **no repository code**. It downloads the artifact, checks the sha256, and installs minisign 0.12 from the pinned release URL with a pinned sha256. It fetches `keys/minisign-{primary,backup}.pub` at the release commit through the API, then runs `minisign -S -t "local-ai-chat <tag>"`. It checks the result with `minisign -Vm` against the committed public keys, including the exact trusted comment, and uploads artifact `release-sig`. It fails if the secret or the public keys are missing. |
| `publish` | ubuntu | `contents: write`, `id-token: write`, `attestations: write` | Checks the sha256 and that all 3 files exist, then runs `actions/attest-build-provenance` on the exe. Deletes stale drafts of the same tag, then runs `gh release create --draft` (`--target <sha>` on `main`, `--verify-tag` for a tag push) with `local-ai-chat.exe`, `local-ai-chat.exe.sha256` and `local-ai-chat.exe.minisig`. Checks that the draft has exactly those 3 non-empty assets, then `gh release edit --draft=false` and checks the tag points at the commit. |
| `dry-run` | windows | `contents: read`, no secrets | Runs only on manual **Run workflow** (`workflow_dispatch`). It makes a throwaway key, signs the built exe with `local-ai-chat v<version>`, and verifies it with `minisign -Vm`. It also runs the app's verifier (`cargo test ci_dry_run_signature -- --ignored`). It never creates a release. |

Draft safety: `releases/latest` never returns drafts, and 2.2.0 clients reject
drafts. A half-made release is never offered. If `publish` fails after creating
the draft, re-run the failed jobs: the stale draft is deleted first.

Asset names are fixed. 2.2.0 and older require both `local-ai-chat.exe` and
`local-ai-chat.exe.sha256`, and refuse the release without them.

## Key generation (no PC needed)

The owner uses many machines and keeps nothing on them, so the keys are made in
a throw-away **GitHub Codespace**. The steps are in Arabic in `RELEASING.md`. In
short:

1. Create the `release` Environment. Set the owner as **required reviewer**, and
   set **Deployment branches and tags** to branch `main` plus tags `v*`. The tag
   ruleset restricts tag updates and deletions but not creations, since CI
   creates the tag.
2. Open a Codespace on the repo from the browser and install `minisign`. Run
   `minisign -G` twice, outside the repo folder: once for `primary` and once for
   `backup`, each with a strong password.
3. Put the primary secret key file content into Environment secret
   `MINISIGN_SECRET_KEY`, and its password into `MINISIGN_PASSWORD`.
4. Put the backup secret key file content and its password into the owner's
   (cloud) password manager. They go nowhere else.
5. Commit `keys/minisign-primary.pub` and `keys/minisign-backup.pub`, and send
   both to us. A follow-up PR pastes them into `TRUSTED_PUBLIC_KEYS`.
6. Delete the secret files and the Codespace.

The release workflow needs steps 1 to 5 done before it can publish. Without them,
`sign` fails with a pointer to this file.

## Rotation and loss

minisign files carry one signature, and a client trusts only the keys compiled
into the version it is running. So a release must be signed by a key that **every
client still in use** trusts. Keep rotations rare.

- **Primary key lost or leaked.** The backup key signs, and the new key set ships
  in that same release:
  1. Make a new **backup** pair in a Codespace (as in "Key generation").
  2. Put the **old backup** secret key and password into the `release`
     Environment secrets (`MINISIGN_SECRET_KEY`, `MINISIGN_PASSWORD`). It is now
     the signing key.
  3. Release a version with `TRUSTED_PUBLIC_KEYS = [old backup, new backup]`,
     dropping the lost key, and update `keys/*.pub` to match. Every client
     already trusts the old backup, so every client accepts this release. From
     then on, updated clients stop trusting the lost key.
  4. Put the new backup secret key in the password manager only. If the key
     leaked, also rotate GitHub credentials and review recent releases. Clients
     that have not updated yet still trust the leaked key, so act fast.
- **Planned rotation** (no loss). Release N ships `[old primary, new key, backup]`
  and is still signed by the old primary. Later releases can be signed by the new
  key. But a client that skips release N, still on N-1, will refuse them and has
  to download once by hand. So keep signing with the old primary for a good while
  after N.
- **Backup key lost.** It is not used for signing. Ship `[primary, new backup]` in
  the next release, signed by the primary.
- **Both keys lost.** No release can be verified by existing clients. Users must
  download a build with new keys **manually**, once, from the releases page or
  https://fastkiwi.app/. There is deliberately no remote override.

## Verifying a release yourself

```bash
# Provenance: built by this repo's release workflow from this tag
gh attestation verify local-ai-chat.exe --repo sarmadiq9090-coder/local-ai-chat

# Signature (key from keys/minisign-primary.pub, or the backup key)
minisign -Vm local-ai-chat.exe -p minisign-primary.pub
#   → "Trusted comment: local-ai-chat v2.3.0"

# Checksum
sha256sum -c local-ai-chat.exe.sha256
```
