# BENCHMARK-SUITE

Status: Draft

## Summary
Define a repeatable benchmark suite for core runtime paths so technical-debt refactors (stores, assets, planning, serialization) can be validated with performance evidence.

## Problem
Performance-sensitive refactors are currently evaluated mostly by functional tests and ad-hoc runs. There is no stable benchmark baseline or regression gate.

## Goals
1. Define benchmark scope and metrics for core crates.
2. Provide reproducible local benchmark commands.
3. Track baseline results and compare changes over time.
4. Cover concurrency-heavy scenarios (store access, asset manager, evaluation pipeline).

## Non-Goals
1. CI regression gating in first phase.
2. Exhaustive micro-optimization of every function.

## Scope
1. Benchmark harness selection and layout (`criterion`-based, or equivalent).
2. Core scenarios:
   1. `AsyncMemoryStore` concurrent get/set/list workloads.
   2. `AsyncFileStore` concurrent read/write/remove workloads.
   3. Asset manager hot paths (`get`, apply/evaluate).
   4. Plan construction and query parsing throughput.
3. Output format:
   1. per-benchmark latency/throughput,
   2. machine/environment metadata,
   3. baseline comparison summary.

## Suggested Milestones
1. Phase 1: benchmark framework + store benchmarks.
2. Phase 2: asset/interpreter benchmarks.
3. Phase 3: documentation and optional CI integration proposal.

## Acceptance Criteria
1. Benchmarks are runnable with documented commands.
2. At least one concurrency benchmark each for memory and file async stores exists.
3. Baseline capture and comparison workflow is documented.
