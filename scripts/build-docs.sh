#!/usr/bin/env bash
# Builds the documentation site: the three user guides (HTML with and without search, EPUB, PDF),
# the home page and the Rust API reference, assembled into one folder ready for GitHub Pages.
#
#   bash scripts/build-docs.sh [--release] [--out DIR] [--skip-api] [--skip-pdf] [--latex]
#                              [--verify | --verify-only]
#
#   --release   release build: only `ready` chapters, writer notes removed; every tool is required
#   --out DIR   the site folder (default target/docs-site)
#   --skip-api  do not run `cargo doc` (the slowest step)
#   --skip-pdf  do not build PDFs
#   --latex     build PDFs through LaTeX (xelatex) instead of Typst
#   --verify    check the assembled site has every expected file, then exit non-zero if not
#   --verify-only  only that check, over a site built earlier into --out
#
# Steps: liquers-docs writes the build tree (target/docs-build); mdBook renders each book twice
# (search on, and search off for the offline zip); Pandoc renders EPUB and PDF; cargo doc renders
# the API. A missing mdbook, pandoc or typst skips its step with a notice, except under --release.
# See specs/guides/USER_DOCS_EDITING_GUIDE.md and specs/guides/GITHUB_ACTIONS_GUIDE.md.

set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
RELEASE=0
OUT="$ROOT/target/docs-site"
SKIP_API=0
SKIP_PDF=0
PDF_ENGINE=typst
VERIFY=0
VERIFY_ONLY=0

while [ $# -gt 0 ]; do
    case "$1" in
        --release) RELEASE=1 ;;
        --out) OUT=$(mkdir -p "$2" && cd "$2" && pwd); shift ;;
        --skip-api) SKIP_API=1 ;;
        --skip-pdf) SKIP_PDF=1 ;;
        --latex) PDF_ENGINE=xelatex ;;
        --verify) VERIFY=1 ;;
        --verify-only) VERIFY=1; VERIFY_ONLY=1 ;;
        -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
        *) echo "build-docs.sh: unknown option $1" >&2; exit 2 ;;
    esac
    shift
done

TREE="$ROOT/target/docs-build"
WORK="$ROOT/target/docs-work"
SKIPPED=()

# have TOOL: true when TOOL is installed; otherwise a notice, or an error under --release.
have() {
    if command -v "$1" >/dev/null 2>&1; then
        return 0
    fi
    if [ "$RELEASE" = 1 ]; then
        echo "build-docs.sh: $1 is required for a release build" >&2
        exit 1
    fi
    echo "notice: $1 is not installed; skipping what needs it" >&2
    SKIPPED+=("$1")
    return 1
}

if [ "$VERIFY_ONLY" = 0 ]; then

echo "== liquers-docs build" >&2
mode=()
[ "$RELEASE" = 1 ] && mode=(--release)
cargo run -q -p liquers-docs -- build --root "$ROOT" --out "$TREE" "${mode[@]}"

rm -rf "$OUT" "$WORK"
mkdir -p "$OUT/guides" "$OUT/downloads" "$WORK"
touch "$OUT/.nojekyll"

# name<TAB>slug<TAB>title, one line per guide, written by liquers-docs.
mapfile -t GUIDES < "$TREE/guides.tsv"

if have mdbook; then
    echo "== mdbook" >&2
    # mdBook empties its destination, so the home book is built aside and copied in.
    mdbook build "$TREE/home" -d "$WORK/home"
    cp -r "$WORK/home/." "$OUT/"
    for line in "${GUIDES[@]}"; do
        IFS=$'\t' read -r name slug title <<< "$line"
        mdbook build "$TREE/$name" -d "$OUT/guides/$name"
        MDBOOK_OUTPUT__HTML__SEARCH__ENABLE=false \
            mdbook build "$TREE/$name" -d "$WORK/$slug-offline"
        (cd "$WORK" && zip -qr "$OUT/downloads/$slug-offline.zip" "$slug-offline")
    done
fi

if have pandoc; then
    echo "== pandoc" >&2
    for line in "${GUIDES[@]}"; do
        IFS=$'\t' read -r name slug title <<< "$line"
        pandoc "$TREE/$name.md" --from markdown --toc --split-level=1 \
            --resource-path="$TREE/$name/src" -o "$OUT/downloads/$slug.epub"
        if [ "$SKIP_PDF" = 0 ] && have "$PDF_ENGINE"; then
            pandoc "$TREE/$name.md" --from markdown --toc --pdf-engine="$PDF_ENGINE" \
                --resource-path="$TREE/$name/src" -o "$OUT/downloads/$slug.pdf"
        fi
    done
fi

{
    echo '<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">'
    echo '<meta name="viewport" content="width=device-width, initial-scale=1">'
    echo '<title>Liquers downloads</title></head><body><h1>Downloads</h1><ul>'
    for line in "${GUIDES[@]}"; do
        IFS=$'\t' read -r name slug title <<< "$line"
        echo "<li>$title:"
        for ext in epub pdf; do
            [ -f "$OUT/downloads/$slug.$ext" ] && echo " <a href=\"$slug.$ext\">${ext^^}</a>"
        done
        [ -f "$OUT/downloads/$slug-offline.zip" ] && echo " <a href=\"$slug-offline.zip\">offline HTML</a>"
        echo "</li>"
    done
    echo '</ul><p><a href="../index.html">Home</a></p></body></html>'
} > "$OUT/downloads/index.html"

if [ "$SKIP_API" = 0 ]; then
    echo "== cargo doc" >&2
    cargo doc -q --no-deps --workspace --exclude liquers-py --exclude liquers-web
    mkdir -p "$OUT/api"
    rm -rf "$OUT/api/rust"
    cp -r "$ROOT/target/doc" "$OUT/api/rust"
fi

else
    mapfile -t GUIDES < "$TREE/guides.tsv"
    # What the build skipped is visible from the site itself.
    [ -d "$OUT/api/rust" ] || SKIP_API=1
    compgen -G "$OUT/downloads/*.pdf" > /dev/null || SKIP_PDF=1
fi

if [ "$VERIFY" = 1 ]; then
    echo "== verify" >&2
    missing=0
    expect() {
        if [ ! -e "$OUT/$1" ]; then
            echo "missing: $1" >&2
            missing=1
        fi
    }
    expect index.html
    expect guides/index.html
    expect downloads/index.html
    for line in "${GUIDES[@]}"; do
        IFS=$'\t' read -r name slug title <<< "$line"
        expect "guides/$name/index.html"
        if ! compgen -G "$OUT/guides/$name/searchindex*.js" > /dev/null; then
            echo "missing: guides/$name/searchindex*.js (search is on in the online copy)" >&2
            missing=1
        fi
        expect "downloads/$slug-offline.zip"
        if [ -f "$OUT/downloads/$slug-offline.zip" ] \
            && unzip -l "$OUT/downloads/$slug-offline.zip" | grep -q 'searchindex'; then
            echo "unexpected: the offline copy of $name has a search index" >&2
            missing=1
        fi
        expect "downloads/$slug.epub"
        [ "$SKIP_PDF" = 0 ] && expect "downloads/$slug.pdf"
    done
    [ "$SKIP_API" = 0 ] && expect api/rust/liquers_core/index.html
    if [ "$missing" = 1 ]; then
        exit 1
    fi
    echo "verify: ok" >&2
fi

if [ ${#SKIPPED[@]} -gt 0 ]; then
    echo "notice: skipped (not installed): ${SKIPPED[*]}" >&2
fi
echo "site: $OUT"
