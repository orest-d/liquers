// STUBS02 for the `records` feature — correct usage of the record-stream declarations.
//
// Kept apart from valid_usage.ts because these declarations exist only in a build with the
// `records` feature (off by default). scripts/check-stubs.sh type-checks this file when it is run
// with `--records` against such a build.

import init, * as liquers from '../../examples-web/quickstart/dist/liquers_web.js';

async function main(): Promise<void> {
  await init();

  // A record source: listed without evaluation, streamed one batch at a time, or materialized.
  const source = (await liquers.evaluate('rows/ns-rec/from_json-records/ns-rec/to_record_source')) as liquers.RecordSource;
  const firstChunk: liquers.LiquersRecordChunk | undefined = source.chunks()[0];
  void firstChunk;
  let streamedRows = 0;
  for await (const batch of await source.stream()) {
    streamedRows += batch.numRows;
    batch.free();
  }
  const whole: liquers.RecordBatch = await source.materialize(100000);
  void (whole.numRows === streamedRows);

  // RecordBatch / RecordColumn. `RecordBatch` has no public constructor — a page gets one back
  // from a query that evaluates to a table — so this checks the declared shape via a type
  // assertion rather than constructing one.
  const batch = null as unknown as liquers.RecordBatch;
  const numRows: number = batch.numRows;
  const numColumns: number = batch.numColumns;
  const schemaJson: string = batch.schemaJson();
  const descriptor: liquers.LiquersRecordColumnDescriptor = batch.column(0);
  const columnCopyResult: unknown = batch.columnCopy(0);
  void numRows;
  void numColumns;
  void schemaJson;
  void columnCopyResult;

  const kind: string = descriptor.kind;
  const validity: liquers.LiquersRecordColumnDescriptor | null = descriptor.validity;
  void kind;
  void validity;

  const recordColumn = null as unknown as liquers.LiquersRecordColumn;
  const view: ArrayBufferView = recordColumn.view;
  const copy: liquers.LiquersRecordColumnCopy = recordColumn.toCopy();
  void view;
  void copy;
}

void main;
