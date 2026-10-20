// The async-iterator companion for `RecordSource.stream()` (liquers-web/src/records.rs).
//
// `stream()` resolves to `asyncIterable(recordStream)`, so a page can write
//
//     for await (const batch of await source.stream()) { ... }
//
// `next()` asks the Rust `RecordStream` for its next batch; at the end, or when the loop is left
// early (`break`, `return`, a throw — all of which call `return()`), the stream handle is freed
// exactly once. The batches it yielded are the page's to free.

export function asyncIterable(stream) {
  let live = true;
  const release = () => {
    if (live) {
      live = false;
      stream.free();
    }
  };
  return {
    [Symbol.asyncIterator]() {
      return this;
    },
    async next() {
      if (!live) {
        return { done: true, value: undefined };
      }
      let batch;
      try {
        batch = await stream.nextBatch();
      } catch (error) {
        release();
        throw error;
      }
      if (batch === null || batch === undefined) {
        release();
        return { done: true, value: undefined };
      }
      return { done: false, value: batch };
    },
    async return(value) {
      release();
      return { done: true, value };
    },
  };
}
