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

  // The execution context, in the position of an argument named `context`.
  liquers.registerCommand({
    name: 'report',
    async: true,
    run: async (context: liquers.Context) => {
      context.info('fetching');
      const where: number | null = context.element;
      if (context.isCancelled()) return String(where);
      return await context.evaluate('hello');
    },
  });
  liquers.registerCommand({
    name: 'plain_context',
    arguments: [{ name: 'context' }],
    conventions: { context: false },
    run: (context: string) => context,
  });

  // A mounted UI.
  const app = document.createElement('div');
  const ui: liquers.UIMount = await liquers.mountUI(app, { query: 'hello/q/ns-lui/query_console' });
  const sub: liquers.UISubscription = ui.on('status', (e: liquers.UIStatusEvent) => {
    const h: number | null = e.handle;
    if (e.error) console.error(e.error.message, h, e.status);
  });
  const shown: number = ui.show('hello');
  ui.show('hello', shown);
  ui.submit('hello', shown);
  for (const info of ui.elements()) {
    const name: string | null = info.typeName;
    console.log(info.handle, info.parent, name, info.title, info.status);
  }
  const running: boolean = ui.running;
  sub.dispose();
  if (running) ui.dispose();
  await liquers.mountUI(app, { spec: 'layout: !vertical null' });

  // A page-defined element, shown in a mount or drawn by the page's own renderer.
  const gauge: liquers.ElementDefinition = {
    type: 'gauge',
    title: 'Gauge',
    render: (p) => `<span>${p.value}%</span><button data-lq-event="inc">+</button>`,
    onEvent: (p, e: liquers.UIUserEvent) => (e.kind === 'inc' ? { ...p, value: p.value + 1 } : undefined),
    mount: (node: Element, p) => {
      node.setAttribute('data-v', String(p.value));
      return () => node.removeAttribute('data-v');
    },
  };
  liquers.defineElement(gauge);
  const made: liquers.UIElement = liquers.element('gauge', { value: 50 }, 'CPU');
  const kind: string | undefined = made.kind;
  const props: any = made.props;
  void kind;
  void props;
  const renderer: liquers.UIRenderer = {
    render(tree: liquers.UITree, changes: liquers.UIChangeRecord[] | 'all') {
      for (const root of tree.roots) {
        const node: liquers.UITreeElement = tree.elements[root];
        console.log(node.html, node.children, node.kind, changes === 'all');
      }
    },
  };
  const drawn: liquers.UIMount = await liquers.mountRenderer(renderer, { query: 'gauge-50' });
  drawn.dispatchEvent(1, 'inc', { note: 'x' });
  drawn.dispose();
  const removed: boolean = liquers.undefineElement('gauge');
  void removed;

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

  // ENVIRON08 — a batch: declarations, or imported metadata bound to functions by name. The
  // report is returned, and also carried by the error of a refused batch.
  const report: liquers.LiquersIssueReport = liquers.validateCommands(
    'commands:\n  - name: imported\n',
    { imported: () => 'imported' },
  );
  const firstIssue: liquers.LiquersIssue | undefined = report.issues[0];
  const where: string | undefined = firstIssue?.command?.name;
  void where;
  if (!report.hasErrors) {
    liquers.registerCommands([
      { name: 'one', run: () => 1 },
      { name: 'two', state: 'text' },
    ], { two: { run: (text: string) => text, state: 'text' } });
  }
  try {
    liquers.registerCommands('commands: []');
  } catch (e) {
    if (e instanceof liquers.LiquersError) {
      const summary: string | null | undefined = e.report?.summary;
      void summary;
    }
  }
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

  // The store: raw metadata, and the effective asset info derived from it.
  const store: liquers.Store = liquers.Environment.global().store();
  const assetInfo: unknown = await store.getAssetInfo('data/input.csv');
  const metadata: unknown = await store.getMetadata('data/input.csv');
  void assetInfo;
  void metadata;
  const readOnly: boolean = store.isReadOnly();
  void readOnly;

  // A store entry with the adapter fields, and a page store offering data-only access.
  const entry: liquers.LiquersStoreEntry = {
    type: 'js',
    prefix: 'notes',
    config: { object: 'notes' },
    read_only: false,
    metadata: { storage: 'grouped', on_read: 'media_type-fill/file_facts' },
  };
  void entry;
  const facts: liquers.LiquersDataFacts = { size: 3, modified: '2026-10-15T12:00:00Z', precision: 'seconds' };
  void facts;

  // A page raises a typed error, with and without a key query.
  const notFound = new liquers.LiquersError('key_not_found', 'no such key: a.txt');
  const keyed = new liquers.LiquersError('key_not_found', 'no such key', '-R/data/a.txt');
  const errorType: string = notFound.errorType;
  const categories: string[] = notFound.categories;
  void categories;
  const errorQuery: string | undefined = keyed.query;
  void errorType;
  void errorQuery;

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
