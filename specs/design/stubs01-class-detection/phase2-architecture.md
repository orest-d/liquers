# Phase 2: Solution and Architecture

Replace the `expected_classes=$(grep …)` block and the fallback in `check-stubs.sh` with:

```bash
expected_classes=$(find "$crate/src" -name '*.rs' -print0 | xargs -0 awk '
    /#\[wasm_bindgen\(js_name *= *[A-Za-z_][A-Za-z0-9_]*\)\]/ {
        match($0, /js_name *= *[A-Za-z_][A-Za-z0-9_]*/); s = substr($0, RSTART, RLENGTH)
        sub(/js_name *= */, "", s); pending = s; next }
    /^[[:space:]]*(#\[|\/\/\/|$)/ { next }
    /^[[:space:]]*pub (struct|enum) / { if (pending != "") print pending; pending = ""; next }
    { pending = "" }' | sort -u)
if [[ -z "$expected_classes" ]]; then
    fail "STUBS01 found no #[wasm_bindgen(js_name = …)] classes — detection is broken"
fi
```

Check that `fail` does not exit immediately in this script, and match the existing reporting
style. `awk`'s `match`/`substr` are POSIX.

## Known-issue preflight

None.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-web/scripts/check-stubs.sh` |
| Risk | A `js_name` on an `impl` block (methods) is not followed by `pub struct`, and is cleared by the `{ pending = "" }` rule on the `impl` line. Verify against `src/` (`impl` blocks use `js_class`, not `js_name`). |
| Certainty | High |
