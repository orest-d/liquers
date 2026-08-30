---
id: STORE-OPENDAL-LIST-OPTION-MISPARSED
kind: issue
title: A list-valued store config option is mis-parsed by the OpenDAL path
status: draft
priority: P2
complexity: S
area: [store/backends]
design: opendal-list-option-config
created: 2026-08-29
github:
---
## Problem

`StoreConfig::config_as_string_map` (`liquers-store/src/config.rs`) flattens every configuration
value to a `String` for OpenDAL's `Operator::via_iter`, which takes `(String, String)` pairs. For a
value that is not a JSON string, boolean or number it falls through to `value.to_string()`, which
for a JSON array produces **JSON text**:

```rust
let string_value = match value {
    serde_json::Value::String(s) => s.clone(),
    serde_json::Value::Bool(b) => b.to_string(),
    serde_json::Value::Number(n) => n.to_string(),
    _ => value.to_string(),          // an array becomes `["a","b"]`
};
```

OpenDAL does not read a sequence as JSON. Its `ConfigDeserializer::deserialize_seq`
(`opendal-0.55.0/src/raw/serde_util.rs`) splits the string on **commas**:

```rust
let values = self.1.split(',').map(|v| Pair(self.0.clone(), v.trim().to_owned()));
```

So a configuration written the way a YAML author would naturally write it —

```yaml
- type: opendal_tikv
  prefix: kv
  config:
    endpoints: [ "127.0.0.1:2379", "127.0.0.1:2380" ]
```

— reaches OpenDAL as the single string `["127.0.0.1:2379","127.0.0.1:2380"]` and is split into the
two nonsense elements `["127.0.0.1:2379"` and `"127.0.0.1:2380"]`, quotes and brackets included.
The correct spelling today is the comma-separated string `"127.0.0.1:2379,127.0.0.1:2380"`, which
nothing documents.

Scope is small at present: `endpoints: Option<Vec<String>>` on the `tikv` service is the only
non-scalar field across OpenDAL 0.55's service configs (checked by grep over
`src/services/*/config.rs`). `tikv` is not in `OPENDAL_STORE_TYPES`, so it is reachable only via the
`opendal_tikv` escape hatch. That is why this is P2 rather than higher — but the flattening rule is
general, so any future service gaining a list field inherits the bug silently.

Two adjacent sharp edges in the same function, worth fixing together:

- **Floats.** `serde_json::Number::to_string` on a YAML `1000.0` yields `"1000.0"`, and OpenDAL's
  integer deserializers use `parse::<usize>()`, which rejects it. An author who writes a round
  number with a decimal point gets a parse failure whose message names OpenDAL, not the document.
- **Null.** `serde_json::Value::Null.to_string()` is the literal `"null"`, which OpenDAL will treat
  as the four-character string rather than as an absent value.

Booleans and integers are fine: OpenDAL accepts `"true"`/`"false"` (also `"on"`/`"off"`) and parses
integers from their decimal text, which is exactly what `to_string` produces.

## Impact

An author configuring a list-valued OpenDAL option gets a store that either fails to build with a
confusing message or, worse, builds against nonsense endpoints. There is a workaround — write the
value as a comma-separated string — but it is undocumented and contradicts how every other list in
Liquers configuration is written, including the browser `http` store's `keys`, which genuinely is a
YAML list.

Reach is currently one service that is not in the default type table, so nobody is likely to be hit
today. The cost of leaving it is that the rule is invisible: the next OpenDAL upgrade that adds a
list field to a common service turns this into a live defect with no test to catch it.

## Expected behaviour

`config_as_string_map` should encode a JSON array the way OpenDAL reads one — comma-separated
elements, without JSON quoting — or refuse it with an error naming the option, rather than
producing a string that parses into garbage. Nulls should be omitted rather than stringified, and a
float that is not integral should be refused where an integer is expected.

Whichever is chosen, the encoding rule belongs in `specs/reference/STORE_CONFIG_FSD.md`: it is part
of the configuration format's contract, not an implementation detail.

## Discovery

Found while writing OpenDAL configuration examples for
[`design/store-factories-in-core/`](../design/store-factories-in-core/phase3-examples.md) Phase 3,
by reading OpenDAL 0.55's service config structs and its `ConfigDeserializer` to check which
`StoreArgumentType` each real argument should carry. Not caused by that design, and not fixed by it:
`config_as_string_map` moves to `liquers-core` unchanged, so the behaviour crosses the move intact.

It does inform one of that design's decisions — `StoreArgumentType::Array` describes a *document*
that carries a real list, which is the browser `http` store's `keys`. An OpenDAL list option is
spelled as a comma-separated `String` at the document level, so it is `StoreArgumentType::String`
with the convention stated in its `doc`, not `Array`.
