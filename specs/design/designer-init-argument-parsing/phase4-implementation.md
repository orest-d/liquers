# Phase 4: Implementation Plan

1. Patch `liquers-project/scripts/init_feature.py` (Phase 2). Proof: T1–T5. Agent: haiku tier.
2. Patch `liquers-designer/scripts/init_feature.py` the same way. Proof: T1–T5.
3. Inspect both `validate_phase.py`. Patch them if they use raw `sys.argv`. Proof: `--help` and a
   valid run.
4. `git status` shows no stray `specs/design/` folders. Issue resolution, index. Diff review.
