# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit | `declared_media_type("a.csv", Some("text/csv")) == None` |
| T2 | unit | `declared_media_type("a.bin", Some("image/png")) == Some("image/png")` |
| T3 | unit | `declared_media_type("a.txt", Some("application/octet-stream")) == None`; `(…, None) == None` |
| T4 | HTTP | Multipart upload of `report.csv` → `GET metadata/<base>/report.csv` has `filename == "report.csv"` |
| T5 | HTTP | Store a key with `LegacyMetadata(json!({"x": 1}))` directly via the store, then `GET metadata/<key>` → `{"x": 1}` |
| T6 | HTTP | `GET entry/<key>?format=json` for the same → `metadata == {"x": 1}` |

Tests in `liquers-axum/tests/store_api_routes.rs` (HTTP) and `handlers.rs` `mod tests` (unit).
