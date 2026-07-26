# API Documentation Analysis

Status: DOC-01 and DOC-02 complete  
Last reviewed: 2026-07-26

This folder tracks the analysis and improvement of the Liquers API documentation.
This document is the initial concept inventory, gap assessment, and prioritized
documentation backlog.

## Scope

The audit covers:

- Public Rust APIs in all workspace crates
- Crate-level and module-level rustdoc
- Root and crate READMEs
- Design and implementation documents under `specs/`
- Examples and integration tests where they currently serve as documentation
- The HTTP REST and WebSocket surface
- Python bindings
- Documentation usability for both human developers and coding agents

The primary assessment criteria are:

1. **Correctness**: Does the documentation describe the current implementation?
2. **Discoverability**: Can a reader find the right document from the repository or crate entry point?
3. **Conceptual clarity**: Does it explain the role and boundaries of the concept?
4. **Task completeness**: Can a reader complete a realistic task without reconstructing behavior from source?
5. **Agent usefulness**: Does it provide exact syntax, invariants, signatures, and examples that reduce incorrect code generation?

## Factual verification policy

All API-reference claims must be verified before they are published. Use evidence in
the following order:

1. Public signatures, feature gates, and implementation in the current source tree
2. Crate manifests and workspace configuration
3. Tests and runnable examples
4. Generated rustdoc and compiler output
5. Specifications explicitly marked as current and verified

Specifications, plans, comments, or older reviews are not sufficient evidence when
they conflict with the implementation or have no current-status marker.

For every concept-specific analysis:

- Record the source files and manifests inspected.
- Distinguish verified behavior from interpretation or recommendation.
- Run focused compilation, tests, or rustdoc checks for executable claims.
- Avoid stability, completeness, security, performance, and production-readiness
  claims unless they have direct evidence.
- Verify names, paths, defaults, feature flags, protocol details, and license terms
  against their primary source.
- Record unresolved contradictions as gaps instead of choosing the most plausible
  statement.

The repository `LICENSE` file identifies the project license as the **GNU Affero
General Public License, version 3**. No workspace package currently declares a
Cargo `license` or `license-file` field, so reference documents should link to the
repository license rather than infer package metadata.

## Executive summary

The main problem is not a lack of written material. Liquers has a large amount of
design and implementation documentation, but it lacks a canonical, current,
task-oriented documentation layer.

The current documentation is distributed across rustdoc, `specs/`, source comments,
examples, tests, and development instructions. Design history, future plans, known
limitations, and implemented behavior are frequently mixed together. This makes it
difficult for a human developer to know which material is authoritative and causes
coding agents to infer APIs or behavior from stale specifications.

The documentation work tracked here is **API-reference first**. It prioritizes exact
public types, signatures, contracts, invariants, errors, feature availability, and
cross-links. A separate user guide will be derived from the reference after the
reference is reasonably complete. Task-oriented tutorials are therefore supporting
material, not the organizing structure of this work.

The highest-impact improvements are:

1. Create a canonical API-reference entry point and concept-to-type index.
2. Publish a normative query, key, and action language reference.
3. Publish a consumer-facing asset and execution lifecycle guide.
4. Document environment construction and immediate versus queued execution.
5. Make the command registration guide canonical, discoverable, and consistent.

## Major concept inventory

The API consists of the following major concepts:

1. **System architecture and crate selection**
   - Workspace crate roles and dependency direction
   - Choosing between `liquers-core`, `liquers-lib`, `liquers-store`, `liquers-axum`,
     `liquers-macro`, and `liquers-py`

2. **Query language and parsing**
   - Query syntax, segments, headers, nested queries, templates, parsing, and rendering

3. **Keys, resources, and actions**
   - `Key`, `ResourceName`, resource segments, transform segments, `ActionRequest`,
     namespaces, and realms

4. **Values and serialization**
   - `ValueInterface`, built-in and extended values, type identification, media type,
     data format, and binary serialization

5. **State and metadata**
   - `State`, `Metadata`, `MetadataRecord`, status, errors, logs, progress, filenames,
     and asset information

6. **Commands**
   - Command functions, command executors, registries, metadata, registration macros,
     parameters, injection, defaults, presets, namespaces, realms, and volatility

7. **Recipes**
   - `Recipe`, recipe lists, recipe providers, arguments, links, current working
     directory, expiration, and resource generation

8. **Plans and interpretation**
   - `Plan`, `PlanBuilder`, steps, resolved parameters, execution policies, and the
     interpreter

9. **Environment, session, context, and payload**
   - Global services, environment references, per-action context, users, sessions,
     payloads, and platform-specific environment implementations

10. **Assets and scheduling**
    - `AssetData`, `AssetRef`, asset managers, job queues, fast-track loading,
      immediate and queued evaluation, cancellation, persistence, and notifications

11. **Storage, routing, and caching**
    - Store traits, memory and file stores, OpenDAL, store routers, backend
      configuration, directory operations, persistence, and caches

12. **Dependencies, expiration, and volatility**
    - Dependency records, versions, dependency scheduling, expiration, invalidation,
      cascading, and volatile computations

13. **Errors and diagnostics**
    - `Error`, `ErrorType`, source positions, query/key context, conversion failures,
      and error propagation

14. **HTTP REST and WebSocket APIs**
    - Query, store, asset, and recipe routers; serialization formats; error responses;
      and asset notifications

15. **Python bindings**
    - Python-visible query, metadata, plan, recipe, dependency, expiration, and error
      APIs

16. **Higher-level command and value libraries**
    - Polars, images, UI elements, egui, and web UI

17. **Feature flags and platform behavior**
    - Optional features, default features, native versus WASM behavior, conditional
      `Send`, and immediate versus threaded execution

## Prioritized gap assessment

| Rank | Concept | Human clarity | Coding-agent impact | Status |
|---:|---|---|---|---|
| 1 | Architecture and API-reference navigation | Poor | Very high | Complete |
| 2 | Query language, keys, and actions | Good | Very high | Complete |
| 3 | Assets and execution lifecycle | Medium | Very high | Not started |
| 4 | Environment, context, and end-to-end evaluation | Poor | Very high | Not started |
| 5 | Commands and registration | Medium-good | Very high | Not started |
| 6 | Values, state, metadata, and serialization | Poor | High | Not started |
| 7 | Stores and persistence | Medium | High | Not started |
| 8 | Recipes and plans | Poor-medium | High | Not started |
| 9 | Dependencies, expiration, and volatility | Poor-medium | High | Not started |
| 10 | HTTP and WebSocket APIs | Misleading | High | Not started |
| 11 | Errors and diagnostics | Poor | Medium-high | Not started |
| 12 | Python bindings | Very poor | High for Python work | Not started |
| 13 | Feature flags and platform behavior | Poor | Medium | Not started |
| 14 | Polars, image, and UI libraries | Poor-medium | Medium | Not started |
| 15 | Session and authorization | Poor | Medium now; high for deployment | Not started |

### 1. Architecture and API-reference navigation

This is the highest-leverage documentation improvement.

The [root README](../../README.md) contains only a title and tagline. It does not
allow a developer or coding agent to determine:

- Which crate to depend on
- Whether to use `SimpleEnvironment`, `ImmediateEnvironment`, or
  `liquers_lib::DefaultEnvironment`
- How commands, stores, recipes, and assets are assembled
- How to evaluate a query and retrieve its result
- Which features are stable, optional, experimental, or platform-specific

The [project overview](../PROJECT_OVERVIEW.md) contains useful architecture material,
but it is under `specs/`, mixes current behavior with plans, and is not linked from
the root README.

#### Recommended documentation

- A crate responsibility and dependency table
- A workspace architecture and runtime-relationship map
- A concept index mapped to exact public modules and types
- An environment implementation reference
- A feature and target availability summary
- A glossary linked to detailed API contracts
- A small compiled example used for reference verification, not as the organizing
  structure of a user guide

### 2. Query language, keys, and actions

Queries are the framework's primary user interface, but there is no complete,
normative language reference. The concise description in the
[project overview](../PROJECT_OVERVIEW.md#1-query-language) does not fully specify:

- The grammar and valid segment combinations
- The distinctions among `Key`, `ResourceName`, `Query`, and `QuerySource`
- Action parameter types and escaping
- Nested queries and links
- Relative and absolute resolution
- Segment headers, namespaces, and realms
- Filename and extension semantics
- Canonical rendering and round-tripping
- Invalid inputs and diagnostic positions

The completed [DOC-02 analysis](doc-02-query-language-reference.md) records the
implemented reference, verified semantics, and remaining implementation-level
limitations.

This gap has very high coding-agent impact. Without a normative syntax reference,
agents are likely to generate plausible but invalid query strings, misuse encoding,
or confuse logical keys with filesystem paths.

#### Recommended documentation

- An EBNF-style grammar
- A complete token encoding and decoding table
- Segment and header reference tables
- Key and query resolution rules
- Examples presented as input string, parsed structure, and canonical rendering
- Invalid examples with the expected error and source position
- Compile-tested Rust construction and parsing examples

### 3. Assets and execution lifecycle

Assets have the largest amount of existing conceptual documentation:

- Module-level documentation in [`assets.rs`](../../liquers-core/src/assets.rs)
- The [assets specification](../ASSETS.md)
- The [asset lifecycle map](../ASSET_LIFECYCLE.md)

These documents are valuable, but they emphasize internal channels, scheduler
behavior, historical issues, and source-line references. Public and internal entry
points are mixed together. Some source locations have drifted, and some methods
described in the lifecycle document are currently private.

The public documentation does not answer the most important consumer questions
concisely:

- When should an application call `EnvRef::evaluate`, `AssetManager::get`,
  `get_asset`, `apply`, or immediate evaluation?
- Does a call enqueue work, execute inline, load from a store, or merely return a
  handle?
- When and how is an `AssetRef` shared?
- What do `get`, polling, and subscriptions guarantee?
- Which notifications may be coalesced or missed?
- What is persisted, and when?
- What are the terminal error and cancellation outcomes?
- What is different between the default and immediate asset managers?

#### Recommended documentation

- A short consumer-facing lifecycle guide organized by use case
- A public state-transition table
- A decision table for queued versus immediate evaluation
- Explicit polling and notification guarantees
- Persistence and retry semantics
- Separate public API and implementation-internals sections

### 4. Environment, context, and end-to-end evaluation

The architecture overview explains the hierarchy, but the corresponding public API
has little method-level guidance. `Environment`, its associated types, and most of
its methods in [`context.rs`](../../liquers-core/src/context.rs) are undocumented.

This is especially harmful to coding agents because generic constraints, associated
types, selected asset-manager implementation, payload handling, and initialization
order must all be correct before an application can evaluate a query.

#### Recommended documentation

- The minimum environment required for evaluation
- The roles and ownership boundaries of `Environment`, `EnvRef`, `Context`,
  `Session`, and `Payload`
- The intended application-facing and framework-extension APIs
- Initialization order and lifecycle
- Native and WASM environment choices
- Queued and immediate evaluation examples
- Custom environment and custom payload examples

### 5. Commands and command registration

The [command registration guide](../COMMAND_REGISTRATION_GUIDE.md) is one of the
strongest documents in the repository. It includes a quick reference, macro and
manual registration, generic commands, organization guidance, and examples.

Its main problems are discoverability and conflicting documentation. The
[project overview](../PROJECT_OVERVIEW.md#3-command-system) describes an
attribute-like `#[register_command]` macro, while
[`liquers-macro`](../../liquers-macro/src/lib.rs) exports the function-like
`register_command!` macro. The macro itself has no rustdoc.

#### Recommended documentation

- Make the existing registration guide canonical and link it from crate entry points
- Correct the attribute-like versus function-like macro conflict
- Put the macro DSL reference directly on `register_command!`
- Add compile-tested sync, async, injected, defaulted, enum, first-command, and
  volatile-command examples
- Document realm, namespace, preset, and predecessor resolution
- Explain the relationship between executable registration and command metadata

### 6. Values, state, metadata, and serialization

The three-layer model is introduced clearly in the
[project overview](../PROJECT_OVERVIEW.md#2-three-layer-value-encapsulation), but
its operational contract is missing.

The documentation should define:

- Who owns and mutates metadata
- When a `State` may contain no value
- How `Status`, value presence, and errors relate
- How `type_identifier`, type name, filename extension, media type, and data format
  interact
- Which serialization round trips are guaranteed
- How to implement a custom `ValueInterface`
- When to use the core `Value`, `ExtValue`, or a custom value enum
- Which invariants constructors and persistence code expect

Without these invariants, both humans and agents can write code that compiles but
constructs inconsistent state or metadata.

### 7. Stores and persistence

`liquers-store` configuration has relatively good source documentation and examples
in [`config.rs`](../../liquers-store/src/config.rs). The underlying `Store` and
`AsyncStore` consumer contract is less clear.

#### Recommended documentation

- Logical key versus backend path semantics
- Prefix routing and precedence
- Data and metadata consistency
- Atomicity guarantees
- Directory behavior
- Expected behavior for missing data or metadata
- Concurrency guarantees
- Memory, file, and OpenDAL backend differences
- Environment-variable expansion and supported configuration
- The status and intended future of synchronous store APIs

### 8. Recipes and plans

The relationship between query, recipe, plan, and asset is not documented
operationally enough. The `Recipe` example in the project overview is also
incomplete relative to the current [`Recipe`](../../liquers-core/src/recipes.rs),
which includes circular-dependency and expiration fields.

The canonical flow should be documented as:

```text
Query
  -> recipe resolution
  -> plan building
  -> asset execution
  -> state
  -> optional persistence
```

The guide should explain:

- Key-based recipe resolution
- `cwd` and relative references
- Argument and link overrides
- Placeholders
- Expiration and volatility
- Circular-dependency reporting
- `PlanBuilder` policies and which settings ordinary users should use
- The boundary between stable public API and execution internals

### 9. Dependencies, expiration, and volatility

There is extensive design material, but no concise public contract from which a
developer can reliably derive runtime behavior.

#### Recommended documentation

- What creates a dependency record
- Static versus runtime dependency discovery
- Version generation and comparison
- When an asset becomes expired
- Whether expired data remains readable
- How invalidation propagates
- How volatility differs from immediate expiration
- How recipe expiration combines with command and dependency expiration
- Differences between immediate and queued dependency tracking

This area has high coding-agent impact because caching and re-evaluation code can
look reasonable while violating freshness guarantees.

### 10. HTTP and WebSocket APIs

The [Axum README](../../liquers-axum/README.md) calls the crate
"production-ready" but documents only the query and store APIs. The crate publicly
exports `AssetsApiBuilder` and `RecipesApiBuilder` from
[`lib.rs`](../../liquers-axum/src/lib.rs), but these surfaces are omitted from the
README.

Some routes registered by `AssetsApiBuilder` explicitly return "not implemented"
or "not supported" responses for operations such as asset deletion, modification,
and listing. Recipe metadata is currently a placeholder.

#### Recommended documentation

- An endpoint and capability matrix
- Explicit supported, unsupported, placeholder, and experimental labels
- Request and response schemas
- Binary serialization and content-negotiation rules
- Timeout and cancellation behavior
- HTTP status and Liquers error mapping
- WebSocket request and notification schemas
- WebSocket delivery and reconnection guarantees
- Authentication, authorization, and trusted-network assumptions
- Preferably an OpenAPI description tested against the registered routes

The "production-ready" claim should be removed or qualified until the documented
surface and implementation limitations agree.

### 11. Errors and diagnostics

Document the error taxonomy and how errors flow through:

- Parsing and query positions
- Key and resource resolution
- Plan building
- Command execution
- Value conversion and serialization
- Store operations
- Asset cancellation and terminal failure
- HTTP translation

The documentation should identify which errors are suitable for user display,
which preserve causal context, and which operations may be retried.

### 12. Python bindings

`liquers-py` exposes many Rust concepts from
[`lib.rs`](../../liquers-py/src/lib.rs), but it has no README, API guide, `.pyi`
type stubs, or user-oriented examples.

#### Recommended documentation

- Python package and module map
- Python-visible class, method, and function reference
- Type stubs
- Signatures, defaults, mutability, and return types
- Rust-to-Python naming and type-conversion contracts
- Exception types and error mapping
- Explicit coverage matrix of Rust APIs exposed or absent in Python
- Short tested examples only where needed to clarify an API contract

### 13. Feature flags and platform behavior

The available API changes materially with features such as `egui`, `webui`,
`image-support`, and `polars`. Native and WASM builds also select different asset
managers and async behavior.

Create a compatibility matrix covering:

- Feature
- Default status
- Added modules and types
- Major dependencies
- Native support
- WASM support
- Required runtime
- Example applications

### 14. Higher-level command and value libraries

Polars, image, egui, and UI specifications exist, but their user-facing API
documentation is fragmented.

Each library should document:

- Required feature flags
- Command registration
- Supported value variants
- Serialization formats
- Conversion rules
- A short command catalogue
- One end-to-end example

### 15. Session and authorization

The `Session` abstraction is currently minimal and authorization is largely planned.
Documentation must separate future design from supported behavior.

For HTTP deployments, explicitly document:

- Whether the API performs authentication or authorization
- Where applications must enforce access control
- Whether assets are shared across users
- Which deployment models are considered safe

## Cross-cutting documentation quality findings

### Missing rustdoc

A documentation build with `missing_docs` warnings enabled produced approximately:

| Crate/configuration | Missing-documentation warnings |
|---|---:|
| `liquers-core` | 701 |
| `liquers-lib --no-default-features` | 259 |
| `liquers-axum` | 92 |
| `liquers-store` | 7 |
| `liquers-macro` | 3 |

These are warning instances for the tested configurations, not necessarily unique
public API items. Conditional compilation may affect the totals. The scale is still
representative of the current public API coverage.

`liquers-core` contains only one rustdoc `# Example` section. The other usage
examples are primarily in specs, tests, or development instructions and are
therefore not visible from the generated API reference.

### Broken and stale links

The crate-level glossary in
[`liquers-core/src/lib.rs`](../../liquers-core/src/lib.rs) links to obsolete module
names including:

- `assets2`
- `recipes2`
- `context2`
- `commands2`

The rustdoc build reported 20 warnings, including broken intra-doc links and public
documentation that links to private items.

### Specification status is unclear

Documents under `specs/` may be:

- Current normative behavior
- A design proposal
- An implementation plan
- Historical analysis
- Partially implemented behavior
- A list of resolved or unresolved issues

This status is rarely machine-readable or consistently shown near the document
title. For coding agents, this creates a high risk of implementing an obsolete
design or calling an API that was planned but never made public.

Every specification should carry a standard status block:

```yaml
status: current | proposed | historical | partially-implemented
last-verified: YYYY-MM-DD
implementation:
  - path/to/source.rs
supersedes:
  - optional/older/document.md
```

### Examples are not validated centrally

Critical examples should be compile-tested or executed in CI. `ignore` rustdoc
examples and copied source snippets are likely to drift. Prefer:

- Rustdoc examples that compile
- Small example programs invoked by CI
- Included source files instead of copied snippets
- Route/schema tests for HTTP documentation
- Doctests or integration tests for query examples

## Recommended work order

1. Replace the root README with a canonical API-reference navigation page.
2. Publish the normative query, key, and action language reference.
3. Publish the consumer-facing execution and asset lifecycle guide.
4. Document environment construction and immediate versus queued evaluation.
5. Promote and correct the command registration guide; add macro rustdoc.
6. Define value, state, metadata, and serialization invariants.
7. Document store guarantees and configuration.
8. Document recipe resolution, plan construction, and execution policies.
9. Publish the dependency, expiration, and volatility contract.
10. Correct the Axum claims and publish an endpoint capability matrix.
11. Add Python documentation and generated type stubs.
12. Document errors, features, platforms, and higher-level libraries.
13. Enable documentation-quality checks in CI.

The first five items should produce the largest immediate improvement in coding-agent
performance. They establish the vocabulary, valid syntax, application assembly
pattern, and runtime semantics needed before method-level rustdoc becomes reliably
useful.

## Progress tracker

Use this table as the high-level tracker. Detailed analyses can be added as separate
Markdown files in this folder and linked from the `Analysis` column.

| ID | Concept | Priority | Analysis | Implementation | Verification |
|---|---|---:|---|---|---|
| DOC-01 | Architecture and API-reference navigation | P0 | [Detailed analysis](doc-01-architecture-reference.md) | Complete | Core rustdoc clean; links checked |
| DOC-02 | Query language, keys, and actions | P0 | [Detailed analysis](doc-02-query-language-reference.md) | Complete | Focused parser test, doctests, and rustdoc pass |
| DOC-03 | Assets and execution lifecycle | P0 | Baseline complete | Not started | Not started |
| DOC-04 | Environment and context | P0 | Baseline complete | Not started | Not started |
| DOC-05 | Commands and registration | P0 | Baseline complete | Not started | Not started |
| DOC-06 | Values, state, metadata, serialization | P1 | Baseline complete | Not started | Not started |
| DOC-07 | Stores and persistence | P1 | Baseline complete | Not started | Not started |
| DOC-08 | Recipes and plans | P1 | Baseline complete | Not started | Not started |
| DOC-09 | Dependencies, expiration, volatility | P1 | Baseline complete | Not started | Not started |
| DOC-10 | HTTP and WebSocket APIs | P1 | Baseline complete | Not started | Not started |
| DOC-11 | Errors and diagnostics | P2 | Baseline complete | Not started | Not started |
| DOC-12 | Python bindings | P2 | Baseline complete | Not started | Not started |
| DOC-13 | Feature flags and platforms | P2 | Baseline complete | Not started | Not started |
| DOC-14 | Higher-level libraries | P2 | Baseline complete | Not started | Not started |
| DOC-15 | Session and authorization | P2 | Baseline complete | Not started | Not started |
| DOC-16 | Rustdoc coverage and CI checks | P1 | Baseline complete | Not started | Not started |

## Suggested follow-up analyses

Suggested files for further work:

- `assets-api-analysis.md`
- `commands-api-analysis.md`
- `environment-context-analysis.md`
- `value-state-metadata-analysis.md`
- `store-api-analysis.md`
- `recipes-plans-analysis.md`
- `dependencies-expiration-analysis.md`
- `http-api-analysis.md`
- `python-api-analysis.md`
- `rustdoc-coverage.md`
