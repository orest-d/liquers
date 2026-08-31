# Phase 3: Examples and tests

Add Python tests for `check()` using a temporary `specs` fixture or testable path helper:

1. a guide linking `../reference/X.md` passes when the target exists;
2. a nested API reference linking `../../../liquers-core/src/context.rs` passes;
3. a missing relative target produces an error containing the source path and target;
4. `target.md#heading` checks `target.md`, while `https://` and `#heading` are ignored;
5. an archive document with a dead link is not scanned.

Run the Python test target used for `docs_index.py` if present, then `python3 scripts/docs_index.py --check`. The final command is the integration gate after repairing current documents.
