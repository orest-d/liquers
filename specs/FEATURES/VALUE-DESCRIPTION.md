# VALUE-DESCRIPTION

Status: Draft

## Summary
Define a general, auto-generated value description model that provides lightweight, structured metadata about a value without requiring full value materialization in clients.

The model must work for built-in values and future user-defined values.

## Problem
Different value types need concise descriptions for UI, APIs, and diagnostics, but current metadata is type-specific and inconsistent. There is no single extensible mechanism for describing values across domains.

## Goals
1. Introduce a general value description schema usable for all value types.
2. Auto-generate descriptions during value production/transformation when possible.
3. Keep descriptions extensible for future and user-defined value types.
4. Preserve backward compatibility when description data is missing.

## Non-Goals
1. Replacing the actual value payload.
2. Storing exhaustive domain metadata (only summary-level information).

## Description Model (General)

A value description should contain:
1. `kind` - stable descriptor category (e.g., `image`, `dataframe`, `text`, `bytes`, `custom:<id>`).
2. `summary` - short human-readable summary.
3. `properties` - extensible key/value map for structured fields.
4. `schema_version` - version of description schema.

Suggested representation in metadata:
1. `value_description.kind: String`
2. `value_description.summary: String`
3. `value_description.properties: Map<String, JsonValue>`
4. `value_description.schema_version: String`

## Type-Specific Examples

### Image
Properties should include (among others):
1. `width`
2. `height`
3. `color_type` (when available)
4. `format` (when available)

### DataFrame
Properties should include:
1. `rows`
2. `columns`
3. `fields` (ordered list of field names)
4. optional `dtypes` map/list when available

### Text/Bytes (minimal)
1. text: `chars`, optional `lines`
2. bytes: `size`

### User-Defined Values
1. `kind` should support namespaced custom kinds (`custom:<provider>/<type>`).
2. `properties` accepts arbitrary JSON-compatible fields.
3. Unknown properties must be ignored by consumers that do not understand them.

## Generation Rules
1. Description should be auto-generated where value is created/updated and type info is available.
2. Transformations that change shape/content should refresh the description.
3. If generation is not possible, keep existing description or leave it absent.

## Compatibility and Fallback
1. Existing assets without value description must remain valid.
2. Consumers should treat missing description as "unknown" and compute on-demand only when needed.
3. Schema evolution must be backward compatible via `schema_version` and tolerant parsing.

## API/Consumer Behavior
1. APIs/UIs should prefer value description for quick display.
2. Value description must never be the sole source of truth for core computations.
3. Consumers should not fail if some properties are missing.

## Acceptance Criteria
1. Images include width/height in value description.
2. DataFrames include rows/columns/fields in value description.
3. Unknown/custom value kinds can provide structured descriptions without code changes in generic consumers.
4. Legacy values without descriptions continue to work.
