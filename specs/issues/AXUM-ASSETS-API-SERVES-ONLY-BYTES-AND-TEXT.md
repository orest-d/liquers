---
id: AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT
kind: issue
title: The assets data and entry endpoints serialize with try_into_bytes, so they serve only bytes and text values
status: draft
priority: P2
complexity: S
area: [axum]
design:
created: 2026-09-24
github:
---
# The assets data and entry endpoints serialize with `try_into_bytes`, so they serve only bytes and text values

## Problem

`GET <base>/data/{*query}` and `GET <base>/entry/{*query}` of the assets API (`<base>` is conventionally
`/api/assets`, `AssetsApiBuilder::new`) turn the asset's value into bytes with
`ValueInterface::try_into_bytes`:

- `liquers-axum/src/assets/handlers.rs:74`, in `get_data_handler` (`:23`)
- `liquers-axum/src/assets/handlers.rs:252`, in `get_entry_handler` (`:200`)

`try_into_bytes` is a *conversion*, not serialization. For core's `Value` it accepts only `Bytes`
and `Text` (`liquers-core/src/value.rs:695-701`); `CombinedValue` refuses every extended value outright
(`liquers-lib/src/value/extended.rs:286-291`). So an asset holding a number, a JSON object, a list, a
DataFrame or an image fails with "Failed to serialize asset value", although the same query served
through `GET /q/{*query}` succeeds — that handler uses the asset's binary, produced by the
format-aware `as_bytes(data_format)` (`AssetRef::get_binary` / `poll_binary`,
`liquers-core/src/assets.rs:3416`, `:3560`).

## Impact

Two endpoints disagree about what a value's bytes are, and the assets API cannot serve most value
types at all. A client that switches from `/q` to `/api/assets/data` for the same query gets an error
instead of the data.

## Expected behaviour

Both handlers serialize the way `/q` does — through the asset's binary, which applies the effective
data format and reuses the cached encoding — ideally through one shared function, so the endpoints
cannot drift again. A test per endpoint with a non-text value (an integer, a JSON object) would have
caught this.

## Discovery

Found 2026-09-24 while inventorying serialization call sites in `liquers-axum` for
`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`, whose refactor would also fix it — but this is
a correctness bug that can be fixed on its own, before that design exists.
