---
id: CORE-CONFIGURATION-ERROR-KIND
kind: issue
title: Configuration errors are indistinguishable from general errors
status: draft
priority: P3
complexity: S
area: [core/error, store/config]
design: 
created: 2026-08-29
github:
---
## Problem

`ErrorType` has no value for "the configuration you supplied is wrong", so configuration failures
scatter across kinds chosen for other reasons:

| Failure | Kind today | Where |
|---|---|---|
| Missing required config key | `General` | `StoreConfig::require_config_string` |
| Environment variable not set | `General` | `expand_env_vars` |
| Unclosed `${` | `ParseError` | `expand_env_vars` |
| Document will not parse | `ParseError` | `StoreRouterConfig::from_yaml` / `_json` / `_toml` |
| Unknown store type | `General` | `create_store` |

Three kinds for one class of problem, and two of them are `General`, which carries no information at
all. A caller cannot ask "was this a configuration problem?" without matching on message text.

## Impact

Low today, which is why this is P3: the errors are readable, and a human fixing a `stores.yaml` gets
a message that says what is wrong.

It matters more for **language bindings**, which is the direction the project is heading.
`design/environment-builder/` targets a setup path where a JavaScript or Python host builds an
environment from documents; `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` shows `error_type` is what
crosses that boundary. A host that wants to say "your configuration file is invalid" versus "your
query failed" has nothing reliable to branch on.

`design/store-factories-in-core/` adds paths rather than reducing them: an unclaimed store type
(`NotSupported`), a type known but unavailable in this build (`NotSupported`), and — once
`ArgumentCoverage::Complete` types reject unknown keys — a rejected key (`General`).

## Expected behaviour

Configuration failures should be identifiable by kind. Most likely an `ErrorType::ConfigurationError`
with a matching typed constructor, applied to the missing-key, unset-variable, unknown-key and
unknown-type paths.

**Not obviously a clean win, and the design should weigh it rather than assume it:**

- `NotSupported` for an unknown store type is arguably *better* than a generic configuration kind,
  because it says what kind of wrong the value is. Replacing it may lose information.
- `ParseError` for a document that will not parse is already exact. A configuration kind would be
  less precise there, not more.

So the likely shape is a new kind for the *semantic* configuration failures (missing key, unknown
key, unset variable) while parse and support failures keep the kinds they have — which means this is
a taxonomy decision, not a mechanical substitution.

Adding a variant is cheap and self-checking: `AssetData::classify_persistence_error`
(`liquers-core/src/assets.rs:1723`) matches `ErrorType` exhaustively with no `_` arm, so a new
variant is a compile error until it is classified. It must land in the `NotPersisted` group;
`NonSerializable` means "this value cannot be persisted by nature, and that is not an error", which
would silently excuse a configuration failure.

## Discovery

Found while implementing `Error::parse_error` for
[`design/store-factories-in-core/`](../design/store-factories-in-core/) Phase 4 Step 2. The
maintainer authorized adding a new `ErrorType` if one were needed; assessment concluded none was
needed *for the parse case* — `ParseError` is exact — but that the adjacent configuration paths have
a real gap. Recorded rather than folded in, because it is an error-taxonomy change touching code
this design does not otherwise modify.
