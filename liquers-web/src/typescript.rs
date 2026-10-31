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
   * `"none"`, then the declared arguments in order. An argument (or, when `arguments` is
   * omitted, a parameter) named `context` receives the execution `Context` in its position
   * instead, and is not a query argument.
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
  /**
   * `{ context: false }` makes an argument named `context` an ordinary argument instead of the
   * execution context.
   */
  conventions?: { context?: boolean };
}

/**
 * A command's implementation for `registerCommands`: the function, or the function with the
 * declaration's `state` and `async` fields (same names and defaults as `LiquersCommandSpec`).
 */
export type LiquersCommandImplementation =
  | ((...args: any[]) => any)
  | { run: (...args: any[]) => any; state?: LiquersStateMode; async?: boolean };

/** Implementations by command name, for `registerCommands` and `validateCommands`. */
export type LiquersCommandImplementations = Record<string, LiquersCommandImplementation>;

/**
 * A batch for `registerCommands` and `validateCommands`: one declaration, an array of them, or a
 * JSON or YAML string in the command metadata registry format (`commands: [...]`). A declaration
 * may omit `run` when the implementations object has an entry for its name.
 */
export type LiquersCommandBatch =
  | LiquersCommandSpec
  | Omit<LiquersCommandSpec, "run"> & { run?: LiquersCommandSpec["run"] }
  | Array<LiquersCommandSpec | (Omit<LiquersCommandSpec, "run"> & { run?: LiquersCommandSpec["run"] })>
  | string;

/** One finding of a command validation report. */
export interface LiquersIssue {
  severity: "debug" | "info" | "warning" | "error";
  message: string;
  /** The command the finding is about, or `null` when no name is known. */
  command: { realm: string; namespace: string; name: string } | null;
}

/**
 * A command validation report, from `validateCommands`, `registerCommands`, or the `report`
 * property of the error a refused batch throws.
 */
export interface LiquersIssueReport {
  issues: LiquersIssue[];
  errorCount: number;
  warningCount: number;
  hasErrors: boolean;
  /** The message a refusal throws, or `null` when there is no error. */
  summary: string | null;
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
  /** Refuse every write to this store. Default `false`. */
  read_only?: boolean;
  /**
   * How this store's metadata is stored and shaped. Each value is a `store_config` spec (for
   * example `media_type-fill/file_facts`); a field left out takes its default. Needs a store with
   * data-only access: a `js` object without `getData`, `setData`, `removeData`, `listdirData` and
   * `dataFacts` is refused here when the store is built.
   */
  metadata?: LiquersStoreMetadata;
}

/** A store's metadata section. Every field is optional; see `STORE_CONFIG_FSD.md`. */
export interface LiquersStoreMetadata {
  /** `native` (default), `per_entry`, `per_entry-<layout>`, `grouped`, `none`. */
  storage?: string;
  /** `direct` (default), `atomic`, `transactional`. */
  writes?: string;
  on_default?: string;
  on_validate?: string;
  on_write?: string;
  on_read?: string;
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
  /** Refuse every write to every store. Default `false`. */
  read_only?: boolean;
  /** Central metadata storage for the whole router. */
  metadata?: LiquersStoreMetadata;
}

/**
 * A store implemented by the page, passed to `registerStoreObject`.
 *
 * Only `get` is required. Every other method may be omitted, and calls to it then fail with
 * `key_not_supported` rather than returning a permissive default — a half-implemented store
 * reports that it is half-implemented instead of looking empty. Any method may return its value
 * directly or as a `Promise`; `isSupported` is the exception and must be synchronous.
 *
 * Return `null` or `undefined` from `get` or `getMetadata` to say a key is absent: that becomes
 * `key_not_found`. Throwing signals a *failure* instead, and becomes `key_read_error`. A key that
 * exists but has no metadata to report should return `{}` from `getMetadata`, not `null`.
 *
 * A directory has no data, so `get` says it is absent; when `isDir` then answers `true`, the
 * store reports directory metadata listing the `listdir` children. A thrown error is never
 * treated as a directory.
 */
export interface LiquersStoreObject {
  get(key: string): { data: Uint8Array; metadata?: object } | null | undefined
    | Promise<{ data: Uint8Array; metadata?: object } | null | undefined>;
  getMetadata?(key: string): object | null | undefined | Promise<object | null | undefined>;
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
  /**
   * Data-only access, needed by a `metadata` section on this store's entry: all five of
   * `getData`, `setData`, `removeData`, `listdirData` and `dataFacts`, or none. They read and
   * write data objects only and never touch metadata.
   */
  getData?(key: string): Uint8Array | null | undefined | Promise<Uint8Array | null | undefined>;
  setData?(key: string, data: Uint8Array): void | Promise<void>;
  removeData?(key: string): void | Promise<void>;
  /** Raw names in a directory, including any objects a metadata layer keeps there. */
  listdirData?(key: string): string[] | Promise<string[]>;
  /** `null` for an absent object. `modified` is RFC 3339; `precision` defaults to its digits. */
  dataFacts?(key: string): LiquersDataFacts | null | undefined
    | Promise<LiquersDataFacts | null | undefined>;
  /** Optional; with it, `writes: atomic` can use moves. */
  renameData?(from: string, to: string): void | Promise<void>;
  /** The metadata this store itself keeps, or `null`. Needed by the `native` storage model. */
  getStoredMetadata?(key: string): object | null | undefined | Promise<object | null | undefined>;
  /** `true` when `setData` replaces an object atomically. */
  atomicWrite?: boolean;
}

/**
 * What `mountUI` shows first: the value of `query`, or the UI `spec` (YAML text or an object, as
 * `UISpec` serializes). Not both.
 */
export interface MountOptions {
  query?: string;
  spec?: string | object;
}

/** A status change of one element of a mounted UI. */
export interface UIStatusEvent {
  /** The element, or `null` for a root query with no element to show its error. */
  handle: number | null;
  /** The status name, as `Asset.status()` reports it (`"processing"`, `"ready"`, `"error"`, …). */
  status: string;
  error?: LiquersError;
}

/** One node of a mounted UI's element tree, as `UIMount.elements()` lists it (depth first). */
export interface UIElementInfo {
  handle: number;
  parent: number | null;
  /** `null` while the node waits for its query's value. */
  typeName: string | null;
  title: string;
  status: string;
}

/** What `dataFacts` reports about a data object without reading it. */
export interface LiquersDataFacts {
  size: number;
  modified?: string;
  precision?: "seconds" | "millis" | "micros" | "nanos";
  etag?: string;
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

/**
 * One chunk of a `RecordSource`, as `RecordSource.chunks()` lists it without evaluating it.
 * `query` evaluates to the chunk's rows either way: the chunk's own query when `kind` is
 * `"query"`, a resource query over the stored chunk when it is `"key"`.
 */
export interface LiquersRecordChunk {
  /** `"memory"`: a view held in memory, whose `query` is a label rather than evaluable. */
  kind: "query" | "key" | "memory";
  query: string;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// A command declaration object. Typed as `LiquersCommandSpec` in the generated `.d.ts`.
    #[wasm_bindgen(typescript_type = "LiquersCommandSpec")]
    pub type JsCommandSpecObject;

    /// A batch of commands: declarations or a serialized registry.
    #[wasm_bindgen(typescript_type = "LiquersCommandBatch")]
    pub type JsCommandBatch;

    /// Implementations by command name.
    #[wasm_bindgen(typescript_type = "LiquersCommandImplementations")]
    pub type JsCommandImplementations;

    /// A command validation report.
    #[wasm_bindgen(typescript_type = "LiquersIssueReport")]
    pub type JsIssueReport;

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
