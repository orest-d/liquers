# EGUI-VALUE-RENDERING

Status: Draft

## Summary
Complete egui rendering support for value variants that are currently unimplemented in conversion/render paths.

## Problem
`Metadata` and `CommandMetadata` value rendering in egui still contains `todo!()` paths, causing incomplete UI behavior and possible runtime panics when these variants are displayed.

## Goals
1. Render `Metadata` values in a structured, readable form.
2. Render `CommandMetadata` values in a structured, readable form.
3. Provide stable fallback rendering for unknown/partial substructures.

## Proposed Scope
1. Implement value-to-egui widgets for `Metadata` and `CommandMetadata`.
2. Add compact + expanded render modes (summary/detail).
3. Keep rendering deterministic and non-panicking.

## Acceptance Criteria
1. No `todo!()` remains for these variants in egui rendering path.
2. Rendering works for representative metadata samples.
3. Tests cover both value variants and fallback behavior.
