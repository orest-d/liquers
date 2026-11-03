#!/usr/bin/env bash
# Generates the HTML API reference of the Python package `liquers` with pdoc, from the
# docstrings and text signatures of the package and its native extension.
#
#   bash scripts/build-liquers-py-docs.sh [--out DIR] [module]
#
#   --out DIR  where to write the reference (default target/liquers-py-docs)
#   module     the module to document (default `liquers`; another name exists to test failure)
#
# The package must be importable, so the script needs an environment with it installed:
#
#   - inside an activated virtual environment (VIRTUAL_ENV set, as in scripts/test-liquers-py.sh,
#     which calls this after pytest), it uses that environment as it is;
#   - otherwise it creates (or reuses) target/liquers-py-docs-venv and builds the package into
#     it with `maturin develop` (no test-only features, so the reference shows the public API).
#
# It installs pdoc if absent, runs it, and fails when pdoc fails or when the written page does
# not name every entry of the module's `__all__`. Nothing it writes is committed; the
# documentation site (scripts/build-docs.sh) places it under api/python/.
# Design: specs/design/py-api-reference/.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out="$repo_root/target/liquers-py-docs"
module="liquers"
while [ $# -gt 0 ]; do
    case "$1" in
        --out) out="$2"; shift ;;
        -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
        -*) echo "build-liquers-py-docs.sh: unknown option $1" >&2; exit 2 ;;
        *) module="$1" ;;
    esac
    shift
done

if [ -n "${VIRTUAL_ENV:-}" ]; then
    echo "==> using the active environment $VIRTUAL_ENV" >&2
else
    venv="$repo_root/target/liquers-py-docs-venv"
    if [ ! -x "$venv/bin/python" ]; then
        echo "==> creating virtual environment at $venv" >&2
        "${PYTHON:-python3}" -m venv "$venv"
    fi
    # shellcheck disable=SC1091
    source "$venv/bin/activate"
    python3 -m pip install --quiet --disable-pip-version-check maturin
    echo "==> maturin develop" >&2
    maturin develop --quiet --manifest-path liquers-py/Cargo.toml >&2
fi

python3 -c "import pdoc" >/dev/null 2>&1 \
    || python3 -m pip install --quiet --disable-pip-version-check pdoc

echo "==> pdoc $module -> $out" >&2
rm -rf "$out"
python3 -m pdoc "$module" -o "$out" --docformat markdown

page="$out/$module.html"
[ -f "$page" ] || { echo "build-liquers-py-docs.sh: pdoc wrote no $page" >&2; exit 1; }

# Every public name must have its own entry on the page (pdoc gives each an `id`).
missing=$(python3 - "$module" "$page" <<'EOF'
import importlib, sys
module, page = sys.argv[1], sys.argv[2]
text = open(page, encoding="utf-8").read()
names = getattr(importlib.import_module(module), "__all__", [])
print(" ".join(n for n in names if f'id="{n}"' not in text))
EOF
)
if [ -n "$missing" ]; then
    echo "build-liquers-py-docs.sh: the reference has no entry for: $missing" >&2
    exit 1
fi
echo "==> $page" >&2
