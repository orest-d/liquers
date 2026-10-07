// STUBS02 — a representative sample of correct usage that the type checker must accept.
//
// Every construct here is something the quick start or the documentation shows. If `tsc` rejects
// any of it, the declarations are wrong in a way that would make a correctly-written page fail to
// compile.

import init, * as liquers from '../../examples-web/quickstart/dist/liquers_web.js';

async function main(): Promise<void> {
  // STUBS05 — the async entry points are declared awaitable.
  await init();
  await liquers.init();

  // A source command: the minimal declaration is `name` plus `run`.
  liquers.registerCommand({
    name: 'hello',
    run: () => 'Hello, world!',
  });

  // A transform, with the state mode and inferred arguments.
  liquers.registerCommand({
    name: 'shout',
    state: 'text',
    run: (text: string) => text.toUpperCase(),
  });

  // Explicit arguments, with a typed default.
  liquers.registerCommand({
    name: 'repeat',
    state: 'text',
    doc: 'Repeat the input.',
    label: 'Repeat',
    arguments: [{ name: 'count', type: 'int', default: 2 }],
    run: (text: string, count: number) => text.repeat(count),
  });

  // Async, and namespaced.
  liquers.registerCommand({
    name: 'fetch_json',
    namespace: 'myapp',
    async: true,
    volatile: true,
    arguments: [{ name: 'url', type: 'string' }],
    run: async (url: string) => {
      const response = await fetch(url);
      return await response.json();
    },
  });

  // STUBS04 — `registerCommand` neither wraps nor returns the function, so the declared signature
  // of a command's implementation survives registration. `named` is still callable with its own
  // parameter types; a decorator-style API could not promise that.
  const named = (text: string, count: number): string => text.repeat(count);
  liquers.registerCommand({
    name: 'named',
    state: 'text',
    arguments: [{ name: 'count', type: 'int' }],
    run: named,
  });
  const stillTyped: string = named('a', 2);
  void stillTyped;

  // Evaluation returns a Promise.
  const value: unknown = await liquers.evaluate('hello/shout');
  void value;

  // The asset surface, with its own async methods.
  const asset: liquers.Asset = await liquers.getAsset('hello');
  const status: unknown = await asset.status();
  const state: unknown = await asset.get();
  await asset.cancel();
  void status;
  void state;

  // Introspection: `describeCommand` returns typed metadata or null.
  const info = liquers.describeCommand('repeat');
  if (info !== null) {
    const argumentNames: string[] = info.arguments.map((a) => a.name);
    const inferred: boolean = info.argumentsInferred;
    void argumentNames;
    void inferred;
  }

  // Query and Key wrappers.
  const query: liquers.Query = liquers.Query.parse('hello/repeat-3');
  const encoded: string = query.encode();
  const key: liquers.Key = liquers.Key.parse('data/file.txt');
  void encoded;
  void key;

  // RecordBatch / RecordColumn (the `records` feature). `RecordBatch` has no public constructor
  // — a page gets one back from a query that evaluates to a table — so this checks the declared
  // shape via a type assertion rather than constructing one.
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

  // The store: raw metadata, and the effective asset info derived from it.
  const store: liquers.Store = liquers.Environment.global().store();
  const assetInfo: unknown = await store.getAssetInfo('data/input.csv');
  const metadata: unknown = await store.getMetadata('data/input.csv');
  void assetInfo;
  void metadata;

  // Parameter encoding, and the module-level odds and ends.
  const param: string = liquers.encodeParam('two words');
  const ready: boolean = liquers.isInitialized();
  const v: string = liquers.version();
  liquers.unregisterCommand('named');
  liquers.shutdown();
  void param;
  void ready;
  void v;
}

void main;
