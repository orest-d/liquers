//! TypeScript declarations for the parts wasm-bindgen cannot infer.
//!
//! # Why this is not a hand-written `.d.ts`
//!
//! wasm-bindgen already generates `liquers_web.d.ts` from the exported surface, and that file
//! cannot drift — it is regenerated from the same source on every build. A second, hand-maintained
//! declaration file would drift, silently, and a stale declaration is worse than none: a type
//! checker accepts code that then fails at runtime.
//!
//! What the generator cannot do is see *inside* a `JsValue`. `registerCommand(spec: JsValue)`
//! generates as `spec: any`, which type-checks everything including a declaration with a
//! misspelled field or a missing `run`. So the interfaces below are injected into the generated
//! file via `typescript_custom_section`, and the exported functions take
//! [`typescript_type`][tt]-annotated extern types so their signatures reference them.
//!
//! The result is one generated file, with real types, that stays in step with the Rust source
//! automatically. `STUBS02` and `STUBS06` are only meaningful because of this: with `any`, a
//! deliberately-wrong spec object cannot be rejected.
//!
//! [tt]: https://rustwasm.github.io/wasm-bindgen/reference/attributes/on-rust-exports/typescript_type.html

use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const LIQUERS_TYPES: &'static str = r#"
/** The type of a command argument, as declared in a command specification. */
export type LiquersArgumentType =
  | "string" | "str" | "text"
  | "int" | "integer"
  | "float" | "number"
  | "bool" | "boolean"
  | "any";

/** How the input state is presented to a command's `run` function. */
export type LiquersStateMode =
  /** No state: a *source* command, producing a value rather than transforming one. */
  | "none"
  /** The converted value. */
  | "value"
  /** The state's text form. */
  | "text" | "string"
  /** The state itself. */
  | "state";

/** One declared argument of a command. */
export interface LiquersArgument {
  /** The argument name, as it appears in command metadata. */
  name: string;
  /** Defaults to `"any"` when omitted. */
  type?: LiquersArgumentType;
  /**
   * Bound when the query omits this argument. Must be JSON-representable — a string, number,
   * boolean or null — because the planner resolves it without re-entering JavaScript.
   */
  default?: string | number | boolean | null;
}

/**
 * A command declaration, as passed to `registerCommand`.
 *
 * Only `name` and `run` are required; everything else has a meaningful default.
 */
export interface LiquersCommandSpec {
  /** Required. Must be unique within its namespace; re-registering replaces and warns. */
  name: string;
  /**
   * Required. The implementation. Receives the state first when `state` is anything but
   * `"none"`, then the declared arguments in order.
   */
  run: (...args: any[]) => any;
  /**
   * Declared arguments. When omitted, the argument names are **inferred** from `run`'s parameter
   * list — but only over the shapes where the parse is exact: every parameter a plain identifier,
   * and the count matching `run.length`. A default, rest or destructured parameter is refused with
   * an error naming it, rather than guessed at. Declare this to be sure.
   */
  arguments?: LiquersArgument[];
  /** Defaults to `"none"` — a source command. */
  state?: LiquersStateMode;
  /** Defaults to the root namespace. `"web"` is reserved and refused. */
  namespace?: string;
  realm?: string;
  /** Defaults to `name`. */
  label?: string;
  doc?: string;
  /**
   * When omitted, a returned Promise is awaited anyway. Set `false` to have a returned Promise
   * refused instead of silently becoming the command's value; set `true` to state the intent.
   */
  async?: boolean;
  volatile?: boolean;
}

/** Command metadata, as returned by `describeCommand`. */
export interface LiquersCommandInfo {
  name: string;
  namespace: string;
  realm: string;
  label: string;
  doc: string;
  /** Always an array, empty when the command takes no arguments. */
  arguments: LiquersArgument[];
  /** Whether the argument names were inferred from `run` rather than declared. */
  argumentsInferred: boolean;
  /** `"javascript"` for a command registered from JavaScript. */
  module: string;
  volatile: boolean;
}

/** One store in a store-router configuration. */
export interface LiquersStoreEntry {
  /**
   * The backend. `"localstorage"` and `"js"` are browser-only; `"http"`/`"https"` are served by
   * `fetch` here and by OpenDAL natively, so one document works on both.
   */
  type: "localstorage" | "http" | "https" | "js" | string;
  /** Key prefix this store answers for. Empty matches every key. */
  prefix?: string;
  /** Backend-specific settings. See the per-type fields below. */
  config?: LiquersStoreEntryConfig;
}

/**
 * Backend settings. Which fields apply depends on the entry's `type`; the union is deliberately
 * open, because a store factory may contribute backends this build does not know about.
 */
export interface LiquersStoreEntryConfig {
  /** `localstorage`: entry-name namespace. Must contain no `/`. Defaults to `"liquers"`. */
  namespace?: string;
  /** `localstorage`: byte budget the store enforces itself. Omit for unlimited. */
  quota_bytes?: number;
  /** `http`/`https`: URL the key is appended to, after its routing prefix is removed. */
  url_prefix?: string;
  /** `http`/`https`: the keys this store has. HTTP offers no listing, so it must be told. */
  keys?: string[];
  /** `js`: the name previously passed to `registerStoreObject`. */
  object?: string;
  [key: string]: unknown;
}

/** A store-router configuration: stores are tried in order, first matching prefix wins. */
export interface LiquersStoreConfig {
  stores: LiquersStoreEntry[];
}

/**
 * A store implemented by the page, passed to `registerStoreObject`.
 *
 * Only `get` is required. Every other method may be omitted, and calls to it then fail with
 * `key_not_supported` rather than returning a permissive default — a half-implemented store
 * reports that it is half-implemented instead of looking empty. Any method may return its value
 * directly or as a `Promise`; `isSupported` is the exception and must be synchronous.
 *
 * Return `undefined` from `get` to say a key is absent: that becomes `key_not_found`. Throwing
 * signals a *failure* instead, and becomes `key_read_error`.
 */
export interface LiquersStoreObject {
  get(key: string): { data: Uint8Array; metadata?: object } | undefined
    | Promise<{ data: Uint8Array; metadata?: object } | undefined>;
  getMetadata?(key: string): object | Promise<object>;
  set?(key: string, data: Uint8Array, metadata: object): void | Promise<void>;
  setMetadata?(key: string, metadata: object): void | Promise<void>;
  remove?(key: string): void | Promise<void>;
  removedir?(key: string): void | Promise<void>;
  makedir?(key: string): void | Promise<void>;
  contains?(key: string): boolean | Promise<boolean>;
  isDir?(key: string): boolean | Promise<boolean>;
  listdir?(key: string): string[] | Promise<string[]>;
  /** Synchronous. A `Promise` here is truthy and would claim every key. */
  isSupported?(key: string): boolean;
}
"#;

/// `RecordBatch`/`RecordColumn` declarations — kept in their own `typescript_custom_section` so
/// they contribute nothing to the generated `.d.ts` when this crate's `records` feature is off.
/// wasm-bindgen concatenates every `typescript_custom_section` it finds, so this and
/// [`LIQUERS_TYPES`] both land in the same file.
#[cfg(feature = "records")]
#[wasm_bindgen(typescript_custom_section)]
const LIQUERS_RECORD_TYPES: &'static str = r#"
/**
 * How one `RecordBatch` column is laid out in wasm linear memory, as returned by
 * `RecordBatch.column`. `ptr` is a **byte offset** into `memory.buffer` (the exported
 * `WebAssembly.Memory`, e.g. `(wasm as any).memory` from the instantiated module); `len` is the
 * element count at that pointer — the row count for every kind except `"Vector"`, where it is the
 * total number of `f32`s (`rows * dim`).
 *
 * `"BoolBitmap"` is Arrow-style bit-packed, LSB-first: `ptr`/`len` describe the packed bytes, not
 * one byte per row — the kind's name says so, deliberately, so it cannot be mistaken for a
 * `Uint8Array` of one-byte-per-row values. Unpack it yourself, or call `columnCopy` for an
 * already-unpacked `Uint8Array`.
 *
 * `"Text"`/`"Binary"` carry no top-level `ptr`/`len` at all: read `offsets` (an `"Int32"`-kind
 * descriptor, `numRows + 1` entries, Arrow's variable-length layout) and `data` (a `"Uint8"`-kind
 * descriptor of the concatenated bytes) instead — row `i` is `data[offsets[i]..offsets[i+1]]`.
 *
 * `validity` is a `"Bitmap"`-kind descriptor (Arrow-style, LSB-first, set = valid), or `null` when
 * the column has no nulls at all. Like `"BoolBitmap"`, a `"Bitmap"`'s `len` is the packed **byte**
 * length (`ptr`/`len` always describe `len` elements of `kind` — a `"Bitmap"`/`"BoolBitmap"`
 * element is a byte); `rows` carries the logical bit count separately, since the last byte may
 * hold up to 7 padding bits beyond it.
 *
 * A view built from this descriptor is invalidated by `memory.grow` (the `ArrayBuffer` detaches;
 * the pointer itself never moves) and dangles once the owning `RecordBatch` is `free()`d. See
 * `RecordColumn` below, or re-create the view at the point of use — it is O(1).
 */
export interface LiquersRecordColumnDescriptor {
  kind: "Int" | "UInt" | "Float" | "Date" | "Timestamp" | "BoolBitmap" | "Vector"
      | "Text" | "Binary" | "Int32" | "Uint8" | "Bitmap";
  /** Byte offset into `memory.buffer`. Absent on `"Text"`/`"Binary"` — see `offsets`/`data`. */
  ptr?: number;
  /** Element count of `kind` at `ptr` (a byte, for `"Bitmap"`/`"BoolBitmap"`/`"Uint8"`). Absent
   * on `"Text"`/`"Binary"`. */
  len?: number;
  /** `"Vector"` only: `f32`s per row. */
  dim?: number;
  /** `"BoolBitmap"`/`"Bitmap"` only: the logical bit (row) count — `len * 8` overcounts by up to
   * 7 padding bits in the last byte. */
  rows?: number;
  /** `"Text"`/`"Binary"` only: an `"Int32"`-kind descriptor of the offsets buffer. */
  offsets?: LiquersRecordColumnDescriptor;
  /** `"Text"`/`"Binary"` only: a `"Uint8"`-kind descriptor of the byte data. */
  data?: LiquersRecordColumnDescriptor;
  validity: LiquersRecordColumnDescriptor | null;
}

/**
 * What `RecordBatch.columnCopy` returns: an owned, detached copy — safe to hold across an
 * `await` or a `memory.grow`, at the cost of an O(n) copy `column` avoids.
 *
 * Numeric kinds copy to the matching typed array (`"Int"`/`"Timestamp"` to `BigInt64Array`,
 * `"UInt"` to `BigUint64Array`, `"Float"` to `Float64Array`, `"Date"` to `Int32Array`,
 * `"BoolBitmap"` unpacked to one byte per row in a `Uint8Array`). `"Text"` copies to an array of
 * strings; `"Binary"`/`"Vector"` copy to an array of per-row `Uint8Array`/`Float32Array`. A null
 * cell is `null` in the three array forms; the typed-array forms carry no null distinction (read
 * `column`'s `validity` descriptor for that).
 */
export type LiquersRecordColumnCopy =
  | Float64Array | BigInt64Array | BigUint64Array | Int32Array | Uint8Array
  | Array<string | null> | Array<Uint8Array | null> | Array<Float32Array | null>;

/**
 * A JavaScript-side companion for one `RecordBatch` column, built from its descriptor.
 *
 * Shipped as a documented reference implementation in `liquers-web/src/records_column.js` — copy
 * it into a page, or import it from the package's `snippets/` directory if the build bundled it
 * (see `liquers-web/README.md` for which route this build took). Not exported by this module
 * itself: `RecordBatch` hands back plain descriptor objects, and `RecordColumn` is what a caller
 * wraps one in.
 *
 * `view` re-creates the typed array whenever `memory.buffer` has changed identity (a `memory.grow`
 * detaches the previous `ArrayBuffer`) — the same pointer and length, because wasm growth never
 * relocates existing pages. `toCopy()` is the always-safe, owned-copy fallback.
 */
export interface LiquersRecordColumn {
  readonly view: ArrayBufferView;
  toCopy(): LiquersRecordColumnCopy;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// A command declaration object. Typed as `LiquersCommandSpec` in the generated `.d.ts`.
    #[wasm_bindgen(typescript_type = "LiquersCommandSpec")]
    pub type JsCommandSpecObject;

    /// Command metadata, or `null` when the command is not registered.
    #[wasm_bindgen(typescript_type = "LiquersCommandInfo | null")]
    pub type JsCommandInfo;

    /// A `Promise` resolving to an [`crate::LiquersAsset`].
    #[wasm_bindgen(typescript_type = "Promise<Asset>")]
    pub type AssetPromise;

    /// A store-router configuration object, or a YAML/JSON string holding one.
    #[wasm_bindgen(typescript_type = "LiquersStoreConfig | string")]
    pub type JsStoreConfig;

    /// A store implemented by the page.
    #[wasm_bindgen(typescript_type = "LiquersStoreObject")]
    pub type JsStoreObject;
}

#[cfg(feature = "records")]
#[wasm_bindgen]
extern "C" {
    /// A `RecordBatch` column descriptor, as returned by `RecordBatch.column`.
    #[wasm_bindgen(typescript_type = "LiquersRecordColumnDescriptor")]
    pub type JsRecordColumnDescriptor;

    /// An owned column copy, as returned by `RecordBatch.columnCopy`.
    #[wasm_bindgen(typescript_type = "LiquersRecordColumnCopy")]
    pub type JsRecordColumnCopy;
}
