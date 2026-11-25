#!/usr/bin/env bash
# Builds, signs and notarizes the Mac app, then puts it on GitHub as a release:
# the disk image for new installs, and the update package plus `latest.json`
# that installed apps look for (see src-tauri/src/update.rs). Run it from the
# main folder, on main, with nothing unpushed: the release's tag is put on the
# commit that was built.
#
#   ./scripts/mac-release.sh           ship the version already in the files
#   ./scripts/mac-release.sh --check   only check that everything is set up
#
# To pick a new version and ship it in one go, use `./scripts/release.sh`.
#
# Who signs comes from `.env.release` beside package.json (kept out of git):
#   APPLE_SIGNING_IDENTITY   "Developer ID Application: <name> (<team id>)"
#   APPLE_ID                 the Apple account that notarizes
#   APPLE_TEAM_ID            its team id
# The notarize password is read from the Keychain entry named in
# NOTARIZE_KEYCHAIN_ITEM (default: HarnessNotarize, its name since before the
# app was renamed).
#
# Updates are signed with a key of their own, separate from Apple's: the file
# in UPDATE_KEY_FILE (default: ~/.tauri/tilde-update.key), whose password is in
# the Keychain entry named in UPDATE_KEYCHAIN_ITEM (default: TildeUpdateKey).
# Its public half is in tauri.conf.json. Lose the key and installed apps can't
# update any more: people would have to download the app again by hand.
#
# RELEASE_NOTES=<file> uses that file as the release's notes.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Uncomment if codesign fails with errSecInternalComponent:
# security unlock-keychain ~/Library/Keychains/login.keychain-db

fail() {
  echo "mac-release: $*" >&2
  exit 1
}

if [ -f .env.release ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env.release
  set +a
fi
for name in APPLE_SIGNING_IDENTITY APPLE_ID APPLE_TEAM_ID; do
  [ -n "${!name:-}" ] || fail "$name is not set. Put it in .env.release (see the top of this script)."
done
export APPLE_SIGNING_IDENTITY APPLE_ID APPLE_TEAM_ID
APPLE_PASSWORD="$(security find-generic-password -s "${NOTARIZE_KEYCHAIN_ITEM:-HarnessNotarize}" -w)" \
  || fail "no notarize password in the Keychain entry ${NOTARIZE_KEYCHAIN_ITEM:-HarnessNotarize}."
export APPLE_PASSWORD

UPDATE_KEY_FILE="${UPDATE_KEY_FILE:-$HOME/.tauri/tilde-update.key}"
[ -f "$UPDATE_KEY_FILE" ] || fail "no update key at $UPDATE_KEY_FILE (see the top of this script)."
TAURI_SIGNING_PRIVATE_KEY="$(cat "$UPDATE_KEY_FILE")"
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s "${UPDATE_KEYCHAIN_ITEM:-TildeUpdateKey}" -w)" \
  || fail "no update key password in the Keychain entry ${UPDATE_KEYCHAIN_ITEM:-TildeUpdateKey}."
export TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD

gh auth status >/dev/null 2>&1 || fail "the GitHub command line isn't signed in. Run: gh auth login"

if ! git diff --quiet || ! git diff --cached --quiet; then
  fail "there are uncommitted changes. Commit and push them first, so the tag matches what is built."
fi
[ "$(git branch --show-current)" = main ] || fail "not on main."
git fetch -q origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] \
  || fail "main here and on GitHub differ. Pull or push first, so the tag matches what is built."

if [ "${1:-}" = --check ]; then
  echo "mac-release: ready."
  exit 0
fi

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
NAME="$(node -p "require('./src-tauri/tauri.conf.json').productName")"
TAG="v${VERSION}"
REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner)"
COMMIT="$(git rev-parse HEAD)"
BUNDLE="$ROOT/src-tauri/target/release/bundle"

# The update package is made only here, so everyday builds need no update key.
npm run tauri build -- --bundles dmg,app --config '{"bundle":{"createUpdaterArtifacts":true}}'

# Everything uploaded, under names that carry the version.
OUT="$BUNDLE/upload"
rm -rf "$OUT"
mkdir -p "$OUT"
DMG="$OUT/${NAME}_${VERSION}_aarch64.dmg"
PACKAGE="$OUT/${NAME}_${VERSION}_aarch64.app.tar.gz"
cp "$BUNDLE/dmg/${NAME}_${VERSION}_aarch64.dmg" "$DMG"
cp "$BUNDLE/macos/${NAME}.app.tar.gz" "$PACKAGE"

NOTES="Mac (Apple Silicon). Windows coming soon."
if [ -n "${RELEASE_NOTES:-}" ]; then
  NOTES="$(cat "$RELEASE_NOTES")"
fi

# What installed apps read to learn of this version (the updater's format).
node -e '
  const fs = require("fs");
  const [file, version, notes, sig, url] = process.argv.slice(1);
  const platforms = { "darwin-aarch64": { signature: fs.readFileSync(sig, "utf8").trim(), url } };
  fs.writeFileSync(file, JSON.stringify({ version, notes, pub_date: new Date().toISOString(), platforms }, null, 2) + "\n");
' "$OUT/latest.json" "$VERSION" "$NOTES" "$BUNDLE/macos/${NAME}.app.tar.gz.sig" \
  "https://github.com/$REPO/releases/download/$TAG/$(basename "$PACKAGE")"

FILES=("$DMG" "$PACKAGE" "$OUT/latest.json")
if gh release view "$TAG" >/dev/null 2>&1; then
  gh release upload "$TAG" "${FILES[@]}" --clobber
else
  gh release create "$TAG" "${FILES[@]}" --target "$COMMIT" --title "$NAME $VERSION" --notes "$NOTES"
fi
echo "Released $NAME $VERSION: $(gh release view "$TAG" --json url -q .url)"
