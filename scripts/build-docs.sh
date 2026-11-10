#!/usr/bin/env bash
# Builds the documentation site: the landing page (docs/site/), the three user guides (HTML with
# and without search, EPUB, PDF), the home page (overview.html), the API references (every
# Rust crate, Python, JavaScript) and the browser module itself (pkg/liquers-web/, which the
# examples of the Web guide import), assembled into one folder ready for GitHub Pages.
#
#   bash scripts/build-docs.sh [--release] [--out DIR] [--skip-api] [--skip-pdf] [--latex]
#                              [--verify | --verify-only]
#
#   --release   release build: only `ready` chapters, writer notes removed; every tool is required
#   --out DIR   the site folder (default target/docs-site)
#   --skip-api  do not build the API references or the browser module (the slowest step)
#   --skip-pdf  do not build PDFs
#   --latex     build PDFs through LaTeX (xelatex) instead of Typst
#   --verify    check the assembled site has every expected file, then exit non-zero if not
#   --verify-only  only that check, over a site built earlier into --out
#
# Steps: liquers-docs writes the build tree (target/docs-build); mdBook renders each book twice
# (search on, and search off for the offline zip); Pandoc renders EPUB and PDF; cargo doc renders
# the Rust API of every crate (liquers-web for wasm32), scripts/build-liquers-py-docs.sh the
# Python API (pdoc) and scripts/build-liquers-web-docs.sh the JavaScript API (TypeDoc); the
# optimized `records` build of liquers-web that TypeDoc reads is copied to pkg/liquers-web/. A missing
# tool (mdbook, pandoc, typst, python3, npx, wasm-bindgen, the wasm32 target) skips its step with
# a notice, except under --release. Generated output is never committed: it exists only in --out.
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
        -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
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

# The landing page (docs/site/), over the home book's index.html: mdBook writes its first page
# twice, and the home page stays as overview.html.
echo "== landing page" >&2
cp -r "$ROOT/docs/site/." "$OUT/"

if have pandoc; then
    echo "== pandoc" >&2
    for line in "${GUIDES[@]}"; do
        IFS=$'\t' read -r name slug title <<< "$line"
        pandoc "$TREE/$name.md" --from markdown --toc --split-level=1 \
            --resource-path="$TREE/$name/src" -o "$OUT/downloads/$slug.epub"
        if [ "$SKIP_PDF" = 0 ] && have "$PDF_ENGINE"; then
            # Typst embeds Libertinus Serif; naming it keeps the PDF independent of the fonts
            # installed (and older Pandoc templates fail on an empty font list).
            font=()
            [ "$PDF_ENGINE" = typst ] && font=(-V "mainfont=Libertinus Serif")
            pandoc "$TREE/$name.md" --from markdown --toc --pdf-engine="$PDF_ENGINE" "${font[@]}" \
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
    # Every workspace crate: the native ones in one rustdoc tree, then liquers-web (wasm32-only)
    # documented for its own target and copied into the same tree. Its types from other crates
    # are plain text, not links: rustdoc links only the crates of its own run.
    echo "== cargo doc" >&2
    cargo doc -q --no-deps --workspace --exclude liquers-web
    mkdir -p "$OUT/api"
    rm -rf "$OUT/api/rust"
    cp -r "$ROOT/target/doc" "$OUT/api/rust"
    if rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
        cargo doc -q --no-deps -p liquers-web --target wasm32-unknown-unknown
        cp -r "$ROOT/target/wasm32-unknown-unknown/doc/liquers_web" "$OUT/api/rust/"
        mkdir -p "$OUT/api/rust/src"
        cp -r "$ROOT/target/wasm32-unknown-unknown/doc/src/liquers_web" "$OUT/api/rust/src/"
    elif [ "$RELEASE" = 1 ]; then
        echo "build-docs.sh: the wasm32-unknown-unknown target is required for a release build" >&2
        exit 1
    else
        echo "notice: no wasm32-unknown-unknown target; skipping liquers-web's Rust API" >&2
        SKIPPED+=(wasm32)
    fi
    echo "== Python API (pdoc)" >&2
    if have python3; then
        bash "$ROOT/scripts/build-liquers-py-docs.sh" --out "$OUT/api/python"
    fi
    echo "== JavaScript API (TypeDoc) and the browser module" >&2
    if have npx && have wasm-bindgen; then
        bash "$ROOT/scripts/build-liquers-web-docs.sh" --release --features records --out "$OUT/api/js"
        # The module TypeDoc just read, published as it is: the examples of the Web guide import
        # pkg/liquers-web/liquers_web.js from the site (docs/guides/web/02-installing.md). It
        # loads liquers_web_bg.wasm and snippets/ from its own folder, so they travel together.
        dist="$ROOT/liquers-web/examples-web/quickstart/dist"
        mkdir -p "$OUT/pkg/liquers-web"
        cp "$dist/liquers_web.js" "$dist/liquers_web_bg.wasm" "$dist/liquers_web.d.ts" \
            "$OUT/pkg/liquers-web/"
        if [ -d "$dist/snippets" ]; then
            cp -r "$dist/snippets" "$OUT/pkg/liquers-web/"
        fi
    fi
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
    expect overview.html
    expect images/logo.png
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
    if [ "$SKIP_API" = 0 ]; then
        # Every API page the home page links to.
        while read -r target; do
            expect "$target"
        done < <(grep -o '](api/[^)#]*' "$ROOT/docs/index.md" | sed 's/^](//')
        # The browser module the Web guide's examples import, published beside its reference.
        expect pkg/liquers-web/liquers_web.js
        expect pkg/liquers-web/liquers_web_bg.wasm
        # Whatever the module imports from its own folder (snippets/ in a records build).
        if [ -f "$OUT/pkg/liquers-web/liquers_web.js" ]; then
            while read -r target; do
                expect "pkg/liquers-web/$target"
            done < <(grep -o "^import .* from ['\"]\./[^'\"]*" "$OUT/pkg/liquers-web/liquers_web.js" \
                | sed "s/^.*from ['\"]\.\///")
        fi
    fi
    # Every page the landing page links to, "coming soon" cards included, so removing a card's
    # data-soon never exposes a broken link. Links into api/ only when the API was built.
    while read -r target; do
        case "$target" in
            api/*) [ "$SKIP_API" = 0 ] && expect "$target" ;;
            *) expect "$target" ;;
        esac
    done < <(grep -o 'href="[^"#]*' "$ROOT/docs/site/index.html" | sed 's/^href="//' \
        | grep -v -e '^[a-z]*:' -e '/$' -e '^$' | sort -u)
    if [ "$missing" = 1 ]; then
        exit 1
    fi
    echo "verify: ok" >&2
fi

if [ ${#SKIPPED[@]} -gt 0 ]; then
    echo "notice: skipped (not installed): ${SKIPPED[*]}" >&2
fi
echo "site: $OUT"
