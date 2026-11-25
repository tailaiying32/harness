#!/usr/bin/env bash
# Ships a new version in one go: sets the version, merges that through a pull
# request, then builds, signs, notarizes and publishes it with mac-release.sh.
# Run it from the main folder, on main, with nothing unpushed.
#
#   ./scripts/release.sh <patch|minor|major|x.y.z>
#
# RELEASE_NOTES=<file> is passed on as the release's notes.
#
# Should the build fail after the merge, the new version is already on main:
# fix what failed and run ./scripts/mac-release.sh to ship it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

[ -n "${1:-}" ] || { echo "usage: ./scripts/release.sh <patch|minor|major|x.y.z>" >&2; exit 2; }

# Everything the build needs is checked before anything changes.
./scripts/mac-release.sh --check

VERSION="$(node scripts/set-version.mjs "$1")"
BRANCH="release/v$VERSION"
git switch -c "$BRANCH"
git commit -qam "Set the version to $VERSION"
git push -qu origin "$BRANCH"
gh pr create --base main --head "$BRANCH" --title "Release $VERSION" --body "Sets the version to $VERSION."
gh pr merge "$BRANCH" --merge --delete-branch
git switch -q main
git pull -q --ff-only origin main
git branch -D "$BRANCH" 2>/dev/null || true

./scripts/mac-release.sh
