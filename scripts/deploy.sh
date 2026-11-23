#!/bin/sh
# Publishes app/dist to the gh-pages branch of a GitHub repository (default: the "origin" remote).
# The branch is replaced by a single fresh commit each time, so old 41 MB packs do not pile up in history.
#
#   ./scripts/deploy.sh [remote-url]
set -eu
cd "$(dirname "$0")/.."
REMOTE=${1:-$(git remote get-url origin)}
REV=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)

[ -f app/public/packs/latest.json ] || { echo "No dictionary pack found. Run ./scripts/build.sh first." >&2; exit 1; }
[ -f app/public/core.wasm ] || { echo "No WASM core found. Run ./scripts/build.sh first." >&2; exit 1; }
npm --prefix app run build

OUT=$(mktemp -d "${TMPDIR:-/tmp}/lex-deploy.XXXXXX")
trap 'rm -rf "$OUT"' EXIT
cp -R app/dist/. "$OUT"
touch "$OUT/.nojekyll"
git -C "$OUT" init -q -b gh-pages
git -C "$OUT" add -A
git -C "$OUT" -c user.name="$(git config user.name || echo deploy)" -c user.email="$(git config user.email || echo deploy@localhost)" \
  commit -q -m "Deploy $REV"
git -C "$OUT" push -f "$REMOTE" gh-pages

# Turn on Pages for the branch if the GitHub CLI is available (harmless if it is already on).
SLUG=$(printf '%s' "$REMOTE" | sed -n 's#.*github\.com[:/]\(.*\)#\1#p' | sed 's#\.git$##')
if [ -n "$SLUG" ] && command -v gh >/dev/null 2>&1; then
  gh api -X POST "repos/$SLUG/pages" -f 'source[branch]=gh-pages' -f 'source[path]=/' >/dev/null 2>&1 || true
  echo "Site: https://${SLUG%%/*}.github.io/${SLUG#*/}/"
fi
