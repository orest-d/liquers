# Phase 3: Examples and Tests

## Examples

`endpoints: ["127.0.0.1:2379", "127.0.0.1:2380"]` becomes
`127.0.0.1:2379,127.0.0.1:2380`. Top-level `null` is absent from the output map. A nested array,
object, null member, or value such as `"host,a"` returns a typed Liquers error naming `endpoints`.

## Tests

1. In `liquers-core/src/store_config.rs`, cover string, bool, integer, float preservation;
   top-level null omission; homogeneous and mixed scalar arrays; empty-array rejection; and all rejection
   cases. Verify environment expansion after encoding.
2. In `liquers-store/src/store_factory.rs`, create an operator configuration with a safe list using
   an available test scheme or validate the pair vector before I/O; no TiKV server is required.
3. Deserialize equivalent JSON and YAML `StoreConfig` inputs and assert identical string maps.
4. Validate reference examples against the implemented spelling.

Run focused core/store tests, `cargo test -p liquers-store`, and
`bash scripts/check-build-matrix.sh` for OpenDAL-off compatibility.
