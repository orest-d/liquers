# COMBINED-VALUE-DISCRIMINATION

Status: Draft

## Summary
Improve combined value deserialization so type identifiers drive correct base-vs-extended value decoding.

## Problem
Current combined value deserialization does not consistently use type discriminator/type identifier to select the intended decoding branch, risking ambiguous or incorrect reconstruction.

## Goals
1. Use type identifier as primary discriminator during decode.
2. Provide deterministic fallback rules when discriminator is absent/unknown.
3. Preserve backward compatibility for existing serialized data where feasible.

## Proposed Scope
1. Introduce dispatch table keyed by type identifier.
2. Apply deterministic decode order (known extended, then base fallback).
3. Add roundtrip tests for base and extended value families.

## Acceptance Criteria
1. Extended values deserialize through intended branch when identifier is known.
2. Base values remain decodable with stable behavior.
3. Roundtrip tests validate discriminator-driven behavior.
