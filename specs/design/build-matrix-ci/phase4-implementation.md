# Phase 4: Reproducible Implementation Record

1. Add `.github/workflows/build-matrix.yml` with the documented triggers and one job. Dependency:
   existing matrix script. Proof: workflow parses and appears on a qualifying PR. Containment:
   revert the workflow file.
2. Add checkout, wasm target, native package, cache, and script steps. Dependency: step 1. Proof:
   job reaches and reports all script rows. Containment: remove or pin the failing setup step.
3. Extend trigger paths to `.cargo/**`. Dependency: observed configuration ownership. Proof: path
   review/qualifying change. Containment: revert commit `991adbd` only.
4. Run the matrix and docs-index checks, update the source resolution, and review the diff for
   duplicate row definitions, broad triggers, unpinned actions, secrets, and unrelated edits.
   Containment: documentation/index changes are regenerated or reverted with their source record.

This plan records commits `a9a14c9` and `991adbd`; it was not executed in this design run.
