---
id: AXUM-API-BASE-WITH-TRAILING-SLASH-DOUBLES-THE-SEPARATOR
kind: issue
title: The Assets, Store and Recipes APIs mounted at a base ending in a slash get double-slash routes
status: draft
priority: P3
complexity: S
area: [axum]
design: axum-query-api-base-slash
created: 2026-10-17
github:
---
# The Assets, Store and Recipes APIs mounted at a base ending in a slash get double-slash routes

## Problem

`AssetsApiBuilder`, `StoreApiBuilder` and `RecipesApiBuilder` join their base path to each route
with `format!("{}/data/{{*key}}", self.base_path)` (`liquers-axum/src/store/builder.rs`,
`recipes/builder.rs`, and `assets/builder.rs`'s `{}/ws`). A base written with a trailing slash, the
spelling the Query API's tests use (`"/q/"`), produces `/api/store//data/{*key}`, so the documented
URL `/api/store/data/x` is a 404 and only `/api/store//data/x` matches.

## Problem Example

```rust
let router = StoreApiBuilder::new("/api/store/").build();
// GET /api/store/data/a.txt   -> 404 today; should serve a.txt
// GET /api/store//data/a.txt  -> 200 today; should not be a distinct spelling
```

## Expected behaviour

Every API builder accepts its base with or without a trailing slash and registers the same routes
for both, as `axum-query-api-base-slash` makes the Query API do.

## Discovery

Found on 2026-10-17 while designing `AXUM-QUERY-API-QUERY-KEEPS-LEADING-SLASH` in the backlog
compaction; attached to that design because both are one contract (how a builder's base path joins
its routes).
