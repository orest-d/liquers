# Phase 1: High-Level Design - OpenDAL List Option Configuration Encoding

## Problem and Evidence

`StoreConfig::config_as_string_map` flattens JSON arrays with `Value::to_string()`, producing JSON
text, while OpenDAL reads sequence fields as comma-separated text. Natural YAML list syntax
therefore reaches OpenDAL with brackets and quotes.

## Expected Behaviour and Acceptance Criteria

Flattening either encodes list values as comma-separated OpenDAL text or rejects them with a
Liquers error naming the option. Nulls are omitted or rejected deliberately, and the documented
OpenDAL configuration rule matches implementation.

## Affected Systems

Store configuration and OpenDAL backend construction are affected. Query, command execution, asset
semantics and non-OpenDAL stores should not change.

## Scope and Non-Goals

Scope is `config_as_string_map` and focused configuration tests. This does not add typed per-service
OpenDAL schemas or expand the supported backend table.

## Compatibility, Assumptions and Questions

Comma-joining arrays is ergonomic but only safe for list elements that do not themselves need
commas. Phase 2 must choose encode versus reject and state null/float handling.

## Documentation Assessment

`specs/reference/STORE_CONFIG_FSD.md` already documents the defect and must be updated when the
behaviour changes. No new guide is expected.

## Review

The scope is one conversion function plus reference maintenance. The acceptance criteria are
externally visible through store configuration parsing.
