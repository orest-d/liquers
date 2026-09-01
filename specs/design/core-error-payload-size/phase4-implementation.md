# Phase 4: Reproducible Implementation Record

1. In `liquers-core/src/error.rs`, move fields to `ErrorPayload` and wrap it in boxed `Error`.
   Dependency: none. Proof: size tests. Containment: revert representation and constructors together.
2. Implement transparent serde, deref, payload conversion, and preserve typed constructors.
   Dependency: step 1. Proof: literal JSON/direct-field tests. Containment: keep the old API until
   every compatibility test passes.
3. Add focused size, serde, skipped-field, mutation, and round-trip tests. Dependency: steps 1-2.
   Proof: focused core suite. Containment: tests must fail on the pre-change representation where
   intended and must not encode incidental formatting.
4. Run identical pre/post clippy measurements and all affected crate/binding suites; update source
   resolution and index. Containment: do not land on wire drift or a downstream failure; review for
   unrelated error fixes such as `with_key`.

This plan records commit `42695a0`; it was not executed in this design run.
