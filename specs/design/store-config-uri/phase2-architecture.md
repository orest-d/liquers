Based on `HEAD` plus the approved shape of
[`design/store-factories-in-core/`](../store-factories-in-core/), read rather than remembered.
Nothing here is implemented.

# Phase 2 — Solution and architecture

## Chosen solution: unified URI, normalized before dispatch

A URI entry becomes an ordinary entry before the factory chain sees it. Five steps:

1. **Parse** the URI for its scheme.
2. **Map** the scheme to a store type, first-wins across the chain.
3. **Fill in** `store_type` on the `StoreConfig`, leaving the URI itself in place.
4. **Dispatch** through `ChainedStoreFactory` exactly as a `type:` entry would.
5. **Interpret** the URI inside the claiming factory, which is the only place with the backend
   knowledge to do it.

Step 5 is the one that is not obvious, and §"Why the factory must interpret the URI" explains why the
alternative — a generic normalizer that rewrites a URI into a `config:` map — cannot work.

### Data structures

Two additive fields, both on structs Liquers owns:

```rust
// liquers-core/src/store_config.rs
pub struct StoreConfig {
    #[serde(rename = "type", default)]
    pub store_type: String,          // now defaultable: a URI entry fills it in
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,         // NEW
    pub prefix: String,
    pub config: HashMap<String, serde_json::Value>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

// liquers-core/src/store_factory.rs
pub struct StoreTypeInfo {
    pub store_type: String,
    // … label, doc, arguments, availability, coverage …
    /// URI schemes this store type answers to. Empty means "no URI spelling".
    #[serde(default)]
    pub uri_schemes: Vec<String>,    // NEW
}
```

`uri_schemes` on `StoreTypeInfo` rather than a central table is deliberate and follows the parent
design's principle that **a factory declares what it claims**. The chain assembles the
scheme-to-type mapping by walking `store_types()`, first-wins, the same way it assembles the
supported-type list — so scheme resolution and type resolution cannot disagree, because they come
from one source.

### Rejected alternatives

| Alternative | Why rejected |
|---|---|
| **Named interpreter per entry** (`schemes: opendal` beside `uri:`) | The maintainer's option (b): safer and simpler, but messier and less ergonomic, and it pushes a Liquers-internal detail into every document. Reconsider only if scheme collisions prove common — §"Collision risk" argues they will not. |
| **Hand the URI to `Operator::from_uri`** | Bypasses `ChainedStoreFactory` entirely, destroying the property the parent design exists for: that Liquers decides which factory serves a name. A browser build could never override `http`. Also limited to the 10 schemes in `DEFAULT_OPERATOR_REGISTRY`. |
| **Generic normalizer: rewrite URI into `config:` keys centrally** | Impossible without backend knowledge — see below. |
| **Merge the two namespaces** (a store type *is* a scheme) | Loses the distinction the maintainer asked to keep, and would let an OpenDAL rename silently change a Liquers type name. |
| **A new `create_from_uri` method on `StoreFactory`** | Unnecessary: the URI travels inside `StoreConfig`, which `create` already receives. Avoiding a trait change is what keeps this additive. |

## Why the factory must interpret the URI

A generic normalizer would need one rule for turning a URI's authority and path into configuration
keys. There is no such rule. Read from OpenDAL 0.55's `Configurator::from_uri` implementations:

| Service | authority becomes | path becomes |
|---|---|---|
| `s3` | `bucket` (from the URI *name*) | `root` |
| `gcs` | `bucket` | `root` |
| `ftp` | `endpoint` = `ftp://{authority}` | `root` |
| `webdav` | `endpoint` = `https://{authority}` — **the scheme is hardcoded** | `root` |
| `fs` | *nothing* — no authority at all | `root`, prefixed with `/` |

The authority means a bucket, a host, a host with a synthesized scheme, or nothing, depending
entirely on the backend. **This is backend knowledge, so it belongs in the factory** — which is
exactly where the parent design already puts backend knowledge.

For OpenDAL types the factory does not implement this by hand: it delegates to
`C::from_uri(&OperatorUri::new(uri, extra)?)` using the same `store_type → config type` mapping the
parent design already needs for deriving argument names. That reaches **61 of 62** services, against
the 10 that `Operator::from_uri` resolves — the per-config route is strictly better than the
registry route, which is a second reason not to use OpenDAL's registry.

## What this requires of `store-factories-in-core`

**This section is the design's second purpose.** Verdict first: **no conflict, no change required
before that design is approved.**

| Element of the parent design | Does URI support change it? | Note |
|---|---|---|
| `StoreFactory::store_types()` | **No** | Returns `Vec<StoreTypeInfo>`; the new `uri_schemes` is a field on that struct, not a signature change |
| `StoreFactory::claims()` | **No** | Still keyed on store type; scheme resolution happens before dispatch |
| `StoreFactory::create()` | **No** | Receives `&StoreConfig`, which is where the URI travels |
| `ChainedStoreFactory` first-wins | **No — and it is load-bearing** | The same rule resolves schemes; see below |
| `StoreRouterBuilder` | **Additive** | One normalization pass over the config before the existing build loop |
| `StoreTypeInfo` | **Additive field** | `#[serde(default)] uri_schemes: Vec<String>` |
| `StoreConfig` | **Additive field** | `#[serde(default)] uri: Option<String>`, and `store_type` gains `#[serde(default)]` |
| `ArgumentCoverage`, `StoreTypeAvailability`, `StoreArgumentType` | **No** | Untouched |
| `StoreTypeMap` | **No** | A map-built factory declares `uri_schemes` in the `StoreTypeInfo` it is given |

**Both additions are `#[serde(default)]` fields on Liquers-owned structs**, so adding them later
breaks no document and no implementor. The parent design therefore does **not** need to reserve them
now. Recording that they are anticipated is enough, and this document is that record.

### First-wins is what makes the unified URI safe

The obvious worry is `http`: the browser serves it with `fetch`, OpenDAL serves it natively, and a
URI `http://…` names neither implementation. Under the unified direction the scheme maps to store
type `http` and the chain resolves it — so **the answer is whatever the chain already gives for
`type: http`**, which is the browser factory in a browser build and the OpenDAL factory natively.
The URI form inherits the type form's semantics for free, with no special case.

This is a genuine argument *for* option (a) over the parent design, not merely compatibility with
it: had the parent design kept the old "factories are consulted before built-ins" rule, scheme
resolution would have had no single ordering to appeal to.

### The one real trap: do not give `filesystem` the `fs://` scheme

Harmonization is permitted, and the tempting move is to let core's `filesystem` answer to `fs://`,
since OpenDAL calls the same idea `fs`. **Recommendation: do not.**

Both `liquers-core`'s `filesystem` (an `AsyncFileStore`) and OpenDAL's `fs` are real, both would be
in a native chain, and both would claim `fs://`. First-wins resolves it deterministically to core's —
so a user who wrote `fs://` meaning OpenDAL silently gets a different backend, with no error and no
warning, and the two differ in behaviour (`STORE-OPENDAL-SLASH-HANDLING`, `opendal-path-mapping`).
Deterministic is not the same as unsurprising.

Safer: give `filesystem` the **`file://`** scheme, which OpenDAL does not use, and leave `fs://` to
OpenDAL's `fs`. The namespaces stay distinct, nothing is shadowed, and the mapping table says so
explicitly. Recorded as Phase 1 open question 1, resolved here as a recommendation the gate may
overrule.

### Collision risk, assessed

The maintainer's stated risk was "conflicting URI scheme definitions", judged low. The audit agrees,
with one qualification. Two factories claiming one scheme is **not** a problem — first-wins settles
it, exactly as for store types. The problem is only when the two mean *different things a user could
plausibly intend*, which is the `fs://` case above and, as far as this audit found, only that one.
`memory://` maps to core's `memory` under first-wins and OpenDAL's memory service is unreachable by
URI, which is fine because they are interchangeable in behaviour.

## Data ownership, errors, sync/async

- `uri: Option<String>` is owned; `uri_schemes: Vec<String>` is owned. Nothing large enough for
  `Arc`. No lifetimes.
- Errors stay `liquers_core::error::Error` via typed constructors. New paths: both `type` and `uri`
  present, and neither present, are `Error::general_error` naming the entry's prefix; an unclaimed
  scheme is `Error::not_supported` listing supported schemes, mirroring the unclaimed-type error.
- **Sync throughout**, like the rest of construction. Parsing a URI is not I/O.
- No `unwrap`/`expect`, no `Error::new`, no default match arm.

### `${VAR}` expansion — recommendation

Expand **per configuration value after parsing**, not on the URI string before it. Whole-string
expansion would put an expanded secret inside a query string, so any value containing `&`, `=` or `/`
would need URL-encoding — a new failure mode for exactly the values most likely to contain those
characters. Expanding after parsing keeps today's per-value rule and its tests.

Consequence to document: `${VAR}` inside a URI query string is expanded *after* URL-decoding, so a
secret needs no URL-encoding, but a literal `${` in a URI cannot be escaped. Acceptable; the
`config:` map remains the recommended home for secrets.

## Risk analysis

| Assessment | Record |
|---|---|
| **Files** | `liquers-core/src/store_config.rs` (one field), `liquers-core/src/store_factory.rs` (one field, one normalization function, one error), `liquers-store/src/store_factory.rs` (URI delegation for OpenDAL types), plus colocated tests. Specs: `STORE_CONFIG_FSD.md`, the issue's `status:`, `specs/index.csv`. ~4 source files. |
| **Impact area** | `store/config`, `store/backends`, `core/store`. Downstream: every consumer of `StoreRouterConfig`, but only additively — no existing document changes meaning. |
| **Module/crate reach** | Two crates (`liquers-core`, `liquers-store`); `liquers-web` benefits without editing. Not confined to one module. |
| **Existing-test breakage** | **Expected zero.** Both fields are `#[serde(default)]`, dispatch for `type:` entries is unchanged, and no existing test writes a `uri`. Serialization round-trip tests may need a nudge if `uri: None` is not skipped — hence `skip_serializing_if`. |
| **New validation** | Round-trip equality (a URI entry and its `type` equivalent build the same store); exclusivity errors both ways; unclaimed scheme lists supported schemes; scheme first-wins across a two-factory chain; `${VAR}` inside a query value; the per-service authority mapping for `s3`, `ftp`, `fs`. All offline — OpenDAL construction is verified not to touch the network. |
| **Behavioural risk** | *Compatibility:* additive only. *Data/persistence:* none — configuration is read at startup. *Concurrency:* none; construction is sync and single-threaded. *Performance:* one URL parse per URI entry at startup. *Security:* **the material risk** — a URI invites credentials in one string that gets logged and pasted; mitigated by documenting `config:` + `${VAR}` as the form for secrets, not by code. *Error paths:* three new, all covered above. |
| **Recovery** | Revert two fields and one normalization pass. No migration, no persisted state, no document rewrite. |
| **Certainty** | High on the audit — the parent design's shape is read from its Phase 2, and OpenDAL's `from_uri` behaviour is read from source and confirmed by a run. Open: the `fs://` recommendation, expansion timing, and whether browser types get schemes. None blocks. |

## Relevant open issues

- [`STORE-CONFIG-IN-CORE`](../../issues/STORE-CONFIG-IN-CORE.md) / `store-factories-in-core` —
  **prerequisite.** This design assumes its factory chain, its `StoreTypeInfo`, and its builder. It
  must land first. Not a blocker for *designing*; the audit above is the point.
- [`STORE-OPENDAL-SERVICES-NOT-ENABLED`](../../issues/STORE-OPENDAL-SERVICES-NOT-ENABLED.md) (P0) —
  a URI naming a service that is not compiled in fails the same way a `type:` does. This design
  neither worsens nor fixes it, but URI support would make the defect *more* visible, since a URI is
  the form a newcomer copies from documentation.
- [`STORE-OPENDAL-LIST-OPTION-MISPARSED`](../../issues/STORE-OPENDAL-LIST-OPTION-MISPARSED.md) (P2) —
  **interacts.** A URI query string is already comma-friendly (`?endpoints=a,b`), which is exactly
  the encoding OpenDAL wants, so the URI form sidesteps that defect rather than inheriting it. Worth
  noting in that issue if this lands first.
- [`STORE-OPENDAL-SLASH-HANDLING`](../../issues/STORE-OPENDAL-SLASH-HANDLING.md) /
  `opendal-path-mapping` — no overlap; that design edits `opendal_store.rs`, which this one does not
  touch.

## Critical review

**Against Phase 1.** All six acceptance criteria are addressed: exclusivity (§Data structures,
§errors), prefix orthogonality (unchanged field), equivalence (§Chosen solution step 5 and the
round-trip test), chain-not-registry resolution (§Rejected alternatives), unclaimed-scheme error
(§errors), and `${VAR}` survival (§expansion). Non-goals respected: `type` + `config` stays primary
and nothing exposes `Operator::from_uri`.

**Against the codebase.** `StoreConfig`'s fields, `create_opendal_store`'s use of
`config_as_string_map`, `OperatorUri`'s accessors (`scheme`, `name`, `authority`, `root`, `options`)
and the five `from_uri` implementations tabulated above were all read at their sources. The
registry-versus-config-route counts (10 against 61 of 62) were counted, not estimated. The claim
that construction is offline was confirmed by running it.

**Understated risk, on reflection:** the security note deserves more weight than "documentation
mitigates it". A URI is *designed* to be pasted into chat and shell history in a way a YAML block is
not. Phase 3 should include a guide passage, not merely a reference sentence, and the guide should
show `${VAR}` inside a URI rather than a literal credential in every example.
