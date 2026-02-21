# POLARS-FEATURE-GAPS

Status: Draft

## Summary
Close known functional gaps in Polars integration, mainly separator handling and parquet support path clarity.

## Problem
Polars module has TODO-level gaps around delimiter/separator mapping and parquet capability, limiting practical data IO coverage.

## Goals
1. Support configured separators reliably for CSV-like IO commands.
2. Define and implement parquet support behavior (feature-gated where appropriate).
3. Ensure clear errors when unsupported combinations are requested.

## Proposed Scope
1. Separator normalization/mapping in Polars IO helpers.
2. Parquet read/write integration under explicit Cargo feature policy.
3. Test coverage for delimiter variants and parquet availability/fallback.

## Acceptance Criteria
1. Separator options produce expected parsing/writing behavior.
2. Parquet operations succeed when enabled and fail clearly when disabled.
3. Tests cover both positive and guarded-failure scenarios.
