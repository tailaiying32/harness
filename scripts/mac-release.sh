#!/usr/bin/env bash
# Builds, signs and notarizes the Mac app, then puts its disk image on GitHub
# as a release. Run it from the main folder, on main, with nothing unpushed:
# the release's tag is put on the commit that was built.
#
#   ./scripts/mac-release.sh
#
# It ships the version already in the files. To change it first:
# `node scripts/set-version.mjs <patch|minor|major|x.y.z>`, then commit and
# push that (a pull request, as with any change).
#
# Who signs comes from `.env.release` beside package.json (kept out of git):
#   APPLE_SIGNING_IDENTITY   "Developer ID Application: <name> (<team id>)"
#   APPLE_ID                 the Apple account that notarizes
#   APPLE_TEAM_ID            its team id
# The notarize password is read from the Keychain entry named in
# NOTARIZE_KEYCHAIN_ITEM (default: HarnessNotarize, its name since before the
# app was renamed).
#
# RELEASE_NOTES=<file> uses that file as the release's notes.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Uncomment if codesign fails with errSecInternalComponent:
# security unlock-keychain ~/Library/Keychains/login.keychain-db

if [ -f .env.release ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env.release
  set +a
fi
for name in APPLE_SIGNING_IDENTITY APPLE_ID APPLE_TEAM_ID; do
  if [ -z "${!name:-}" ]; then
    echo "mac-release: $name is not set. Put it in .env.release (see the top of this script)." >&2
    exit 1
  fi
done
export APPLE_SIGNING_IDENTITY APPLE_ID APPLE_TEAM_ID
APPLE_PASSWORD="$(security find-generic-password -s "${NOTARIZE_KEYCHAIN_ITEM:-HarnessNotarize}" -w)"
export APPLE_PASSWORD

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
NAME="$(node -p "require('./src-tauri/tauri.conf.json').productName")"
TAG="v${VERSION}"
DMG="$ROOT/src-tauri/target/release/bundle/dmg/${NAME}_${VERSION}_aarch64.dmg"

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "mac-release: there are uncommitted changes. Commit and push them first, so the tag matches what is built." >&2
  exit 1
fi
COMMIT="$(git rev-parse HEAD)"

npm run tauri build -- --bundles dmg,app

NOTES=(--notes "Mac (Apple Silicon). Windows coming soon.")
if [ -n "${RELEASE_NOTES:-}" ]; then
  NOTES=(--notes-file "$RELEASE_NOTES")
fi

if gh release view "$TAG" >/dev/null 2>&1; then
  gh release upload "$TAG" "$DMG" --clobber
else
  gh release create "$TAG" "$DMG" --target "$COMMIT" --title "$NAME $VERSION" "${NOTES[@]}"
fi
echo "Released $NAME $VERSION: $(gh release view "$TAG" --json url -q .url)"
