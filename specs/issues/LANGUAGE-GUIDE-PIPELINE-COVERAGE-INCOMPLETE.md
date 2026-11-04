---
id: LANGUAGE-GUIDE-PIPELINE-COVERAGE-INCOMPLETE
kind: issue
title: The Language Integration Guide's PIPELINE feature is missing from the profiles, the value bridge and the reference tests
status: draft
priority: P3
complexity: S
area: [docs]
design: 
created: 2026-11-04
github:
---
## Problem

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` gained the `PIPELINE` feature on 2026-11-04 (§4 row,
§5 `### PIPELINE — Pipelines`, tests `PIPELINE01`–`05`), but the rest of the guide was not brought
along. Every other optional feature with prescribed tests is threaded through these places;
`PIPELINE` is in none of them:

- **§4 recommended profiles.** The Browser JavaScript and Python profiles list `DATANODE` and
  `TEMPLATE` among the optional features to select, not `PIPELINE`.
- **The value bridge.** `Value::Pipeline` (`liquers-core/src/value.rs`, `SimpleValue::Pipeline`)
  is a new value variant. §VALUE's contract lists the variants a bridge must cover and does not
  name it, and §PIPELINE lists only the JSON form of a pipeline *argument*, not a pipeline *value*
  returned by a command. In practice both bindings already send it out as its JSON object
  (`liquers-web/src/bridge.rs` `value_to_js` through `try_into_json_value`; `liquers-py/src/value.rs`
  `SimpleValue::Pipeline`), and neither can send one *in*: a native object becomes a map value, not
  a pipeline. Whether that asymmetry is acceptable is a design question the guide does not ask.
- **§6 language-specific guidance.** The Browser JavaScript and Python subsections say nothing
  about pipelines. For JavaScript the relevant fact is the integer bound: a pipeline's argument
  values cross as JSON, and `liquers-web` refuses an outbound integer outside ±(2^53 − 1)
  (`VALUE03`), so a pipeline carrying a large integer cannot be read back.
- **§7 design and review checklist.** No item asks a design selecting `PIPELINE` to state the four
  §PIPELINE questions' answers.
- **Appendix A reference test implementations.** `DATANODE` and `TEMPLATE` have reference
  sections; `PIPELINE` has none, so `PIPELINE01`–`05` have no worked harness.
- **§PIPELINE vs `RECIPE`.** A language recipe provider may return a recipe with a `pipeline`
  (`specs/reference/PIPELINES.md` §5); §RECIPE does not say whether the adapter must accept one.

## Impact

A binding design reading the guide (the planned pipeline work for `liquers-web` and `liquers-py`,
`PIPELINES-NOT-IN-LANGUAGE-BINDINGS`) has to rediscover these points, and the value-bridge
asymmetry is easy to leave unstated. Documentation only; the §PIPELINE section itself is usable.

## Expected behaviour

The guide adds `PIPELINE` to the profiles, names `Value::Pipeline` in §VALUE (with the inbound
question), gives §6 a short pipeline note per language, adds a §7 item, an Appendix A section for
`PIPELINE01`–`05`, and a sentence in §RECIPE. A `## History` row and a `reviewed:` bump go with it.

## Discovery

Checking `liquers-web` against the guide's pipeline requirements on 2026-11-04, at the owner's
request ("there should be the support for pipelines").
