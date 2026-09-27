// The `RecordColumn` companion described in
// specs/design/record-streams/phase2-architecture.md, §"The wasm route: a dedicated safe
// mechanism, not Arrow". Wraps one `RecordBatch.column()` descriptor and re-creates its typed-array
// view whenever `memory.buffer` has changed identity — a `memory.grow` detaches the previous
// `ArrayBuffer`, but the underlying pages never move, so the same pointer and length are still
// correct (Hazard A). See `RecordBatch`'s TypeScript declaration (injected into `liquers_web.d.ts`
// from `src/typescript.rs`) for exactly what a descriptor carries.
//
// `"Text"`/`"Binary"` are variable-length and span two buffers (`offsets`/`data`), so there is no
// single fixed-width view to build for them — wrap `desc.offsets`/`desc.data` in their own
// `RecordColumn`s instead, or call `RecordBatch.columnCopy()` for an already-decoded copy.

/**
 * Builds a typed array over `buffer` at `desc.ptr`, `desc.len` elements of `desc.kind`.
 *
 * @param {ArrayBuffer} buffer
 * @param {{kind: string, ptr: number, len: number}} desc
 * @returns {ArrayBufferView}
 */
function makeView(buffer, desc) {
  switch (desc.kind) {
    case 'Int':
    case 'Timestamp':
      return new BigInt64Array(buffer, desc.ptr, desc.len);
    case 'UInt':
      return new BigUint64Array(buffer, desc.ptr, desc.len);
    case 'Float':
      return new Float64Array(buffer, desc.ptr, desc.len);
    case 'Date':
    case 'Int32':
      return new Int32Array(buffer, desc.ptr, desc.len);
    case 'Vector':
      return new Float32Array(buffer, desc.ptr, desc.len);
    // Bit-packed (`"Bitmap"`/`"BoolBitmap"`) and raw byte data (`"Uint8"`) are all a `Uint8Array`
    // at this layer — `desc.len` is already the packed byte length, per the TypeScript
    // declaration. Unpacking a `"BoolBitmap"`/`"Bitmap"` bit is the caller's job:
    // `(view[i >> 3] >> (i & 7)) & 1`, valid for `i < desc.rows`.
    case 'Uint8':
    case 'Bitmap':
    case 'BoolBitmap':
      return new Uint8Array(buffer, desc.ptr, desc.len);
    case 'Text':
    case 'Binary':
      throw new Error(
        `RecordColumn: "${desc.kind}" has no single fixed-width view (it is two buffers) — wrap ` +
          'desc.offsets and desc.data separately, or use RecordBatch.columnCopy() instead.'
      );
    default:
      throw new Error(`RecordColumn: unknown descriptor kind "${desc.kind}"`);
  }
}

/**
 * A JavaScript-side companion for one `RecordBatch` column descriptor.
 *
 * `memory` is the module's exported `WebAssembly.Memory` (e.g. the `memory` export of the
 * instantiated wasm module); `desc` is whatever `RecordBatch.column(i)` returned.
 */
export class RecordColumn {
  constructor(memory, desc) {
    this.memory = memory;
    this.desc = desc;
    this._view = null;
  }

  /**
   * The typed-array view, re-created (same pointer, same length — Hazard A's whole fix) whenever
   * `memory.buffer` is no longer the buffer the cached view was built over. One reference
   * comparison per access; no data is copied on this path.
   */
  get view() {
    if (this._view === null || this._view.buffer !== this.memory.buffer) {
      this._view = makeView(this.memory.buffer, this.desc);
    }
    return this._view;
  }

  /** The always-safe fallback: a copy detached from linear memory, so it survives a `free()` on
   * the owning `RecordBatch` or being held across an `await`. */
  toCopy() {
    return this.view.slice();
  }
}
