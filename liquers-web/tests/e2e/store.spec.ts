import { test, expect } from '@playwright/test';

/**
 * `STORE07` for all three stores, plus the parts of `STORE02` and `STORE10` that need a real
 * server, plus the reload test.
 *
 * Keyed evaluation first fast-tracks eligible stored values, so these cases cover the complete
 * `env.evaluate('-R/…')` path for plain files rather than only the direct store API.
 *
 * The store surface itself is *not* blocked — `env.store().get(…)` and friends reach all four
 * stores, and the tests that exercise that path run normally.
 *
 * **Why these are here rather than in `wasm-bindgen-test`.** Everything the stores can do without
 * the network is covered in the Node and Chromium suites. What is left needs two things that
 * harness cannot give: an HTTP server that actually serves files with headers, and a page that can
 * be *reloaded*. Playwright already has both, so putting them here costs nothing and removes a
 * fragile dependency.
 *
 * The fixtures are `examples-web/quickstart/fixtures/`, copied into `dist/` by `build.sh` and
 * served by the same static server as the page. Run `build.sh` first.
 */

/**
 * Reaches the module the page already loaded.
 *
 * Deliberately does **not** call the wasm-bindgen initializer: `/index.html` has already run it,
 * and ES modules are cached per URL, so this import yields the very same initialized module.
 * Calling `default()` a second time re-instantiates the wasm module underneath the page's
 * references, which shows up as an intermittent `memory access out of bounds` and then as every
 * later `page.evaluate` hanging — a dead instance never resolves its promises.
 */
const BOOT = `
  const liquers = await import('./liquers_web.js');
  if (!liquers.isInitialized()) await liquers.init();
`;

/** Waits until the page's own script has initialized the wasm module. */
async function awaitReady(page: import('@playwright/test').Page) {
  await page.waitForFunction(() => (globalThis as any).__liquersQuickstart?.ready === true);
}

async function boot(page: import('@playwright/test').Page) {
  await page.goto('/index.html');
  // The page's own script initializes the module; wait for it rather than racing it. Evaluating
  // too early reaches the module before its wasm is instantiated, and every export throws.
  await awaitReady(page);
}

/** Runs an async body in the page with the module bound to `liquers`. */
function inPage(body: string): string {
  return `(async () => {
    ${BOOT}
    ${body}
  })()`;
}

test.describe('STORE — end to end in a real browser', () => {
  test.beforeEach(async ({ page }) => {
    await boot(page);
    // Each test starts from a clean singleton and a clean namespace.
    await page.evaluate(
      inPage(`
        liquers.shutdown();
        await liquers.init();
        for (const k of Object.keys(localStorage)) {
          if (k.startsWith('e2e/')) localStorage.removeItem(k);
        }
        return true;
      `),
    );
  });

  /**
   * STORE07 — a fetched resource evaluates end to end.
   *
   * The whole point of the design in one line: `-R/` works in a browser. Note the `/-/`, without
   * which `to_text` would be read as part of the key rather than as a command.
   */
  test('STORE07 a fetched resource evaluates end to end', async ({ page }) => {
    const out = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore({
          stores: [{
            type: 'http',
            prefix: 'data',
            config: { url_prefix: './fixtures/', keys: ['input.txt', 'sub/report.txt'] },
          }],
        });
        return await env.evaluate('-R/data/input.txt/-/to_text');
      `),
    );
    expect(out).toBe('hello');
  });

  /** STORE07 — a nested key resolves too, so URL construction is not accidentally flat. */
  test('STORE07 a nested fetched key resolves', async ({ page }) => {
    const out = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore({
          stores: [{
            type: 'http',
            prefix: 'data',
            config: { url_prefix: './fixtures/', keys: ['input.txt', 'sub/report.txt'] },
          }],
        });
        return await env.evaluate('-R/data/sub/report.txt/-/to_text');
      `),
    );
    expect(out).toBe('nested');
  });

  /**
   * STORE02 over a real 404.
   *
   * Asserts the *error type*, not merely that it rejected: an HTTP 404 has to become
   * `key_not_found` rather than a read error, because the asset layer treats absence differently
   * from failure.
   */
  test('STORE02 a missing URL is key_not_found', async ({ page }) => {
    const result = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore({
          stores: [{
            type: 'http',
            prefix: 'data',
            config: { url_prefix: './fixtures/', keys: ['nothing-here.txt'] },
          }],
        });
        try {
          await env.store().get('data/nothing-here.txt');
          return { ok: true };
        } catch (e) {
          return { ok: false, errorType: e.errorType };
        }
      `),
    );
    expect(result.ok).toBe(false);
    expect(result.errorType).toBe('key_not_found');
  });

  /**
   * STORE10 — the headers really are read, and the extension really does win.
   *
   * The precedence *rule* is unit-tested; this asserts the store actually reaches a `Response` for
   * both halves. `input.csv` is served as `text/csv` by the static server and must stay `text/csv`;
   * `blob` has no extension, so whatever the server says is what is used.
   */
  test('STORE10 metadata comes from the extension and the response', async ({ page }) => {
    const result = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore({
          stores: [{
            type: 'http',
            prefix: 'data',
            config: { url_prefix: './fixtures/', keys: ['input.csv', 'blob'] },
          }],
        });
        const csv = await env.store().getMetadata('data/input.csv');
        const blob = await env.store().getMetadata('data/blob');
        return { csv: csv.media_type, blob: blob.media_type, size: csv.file_size };
      `),
    );
    expect(result.csv).toBe('text/csv');
    // No extension, so the server's Content-Type is what remains. Any non-empty answer proves the
    // header was read; asserting the exact value would pin the static server's guess, not ours.
    expect(typeof result.blob).toBe('string');
    expect(result.blob.length).toBeGreaterThan(0);
  });

  /**
   * STORE07 — a value written to `localStorage` survives a **reload**.
   *
   * The only test that proves the derived directory index is rebuilt from storage rather than
   * living in the first page's memory. Everything else in the localStorage suite runs inside one
   * page lifetime and would pass with a purely in-memory index.
   */
  test('STORE07 a localStorage resource survives a reload', async ({ page }) => {
    const config = `{ stores: [{ type: 'localstorage', prefix: 'local',
                     config: { namespace: 'e2e' } }] }`;

    await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore(${config});
        await env.store().set('local/in.txt', new TextEncoder().encode('hello'));
        await env.store().makedir('local/empty');
        return true;
      `),
    );

    await page.reload();
    await awaitReady(page);

    const out = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore(${config});
        const text = await env.evaluate('-R/local/in.txt/-/to_text');
        const emptyStillThere = await env.store().isDir('local/empty');
        return { text, emptyStillThere };
      `),
    );
    expect(out.text).toBe('hello');
    expect(out.emptyStillThere, 'an empty directory must survive a reload').toBe(true);
  });

  /** STORE07 — a page-implemented store serves a query. */
  test('STORE07 a JavaScript store evaluates end to end', async ({ page }) => {
    const out = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        env.registerStoreObject('mine', {
          async get(key) {
            if (key !== 'mem/greeting.txt') return undefined;
            return { data: new TextEncoder().encode('from the page'),
                     metadata: { media_type: 'text/plain' } };
          },
          async contains(key) { return key === 'mem/greeting.txt'; },
        });
        await env.configureStore({
          stores: [{ type: 'js', prefix: 'mem', config: { object: 'mine' } }],
        });
        return await env.evaluate('-R/mem/greeting.txt/-/to_text');
      `),
    );
    expect(out).toBe('from the page');
  });

  /**
   * STORE11 — one configuration document, two stores, routed by prefix.
   *
   * The example from Phase 1, executed: read-only reference data from a server alongside writable
   * browser storage, described by a single document.
   */
  test('STORE11 fetch and localStorage coexist in one configuration', async ({ page }) => {
    const out = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore({
          stores: [
            { type: 'http', prefix: 'data',
              config: { url_prefix: './fixtures/', keys: ['input.txt'] } },
            { type: 'localstorage', prefix: 'local', config: { namespace: 'e2e' } },
          ],
        });
        const fetched = await env.evaluate('-R/data/input.txt/-/to_text');
        await env.store().set('local/out.txt', new TextEncoder().encode(fetched + ' world'));
        const stored = await env.evaluate('-R/local/out.txt/-/to_text');

        let readOnlyRefused = null;
        try {
          await env.store().set('data/nope.txt', new TextEncoder().encode('x'));
        } catch (e) {
          readOnlyRefused = e.errorType;
        }
        return { fetched, stored, readOnlyRefused };
      `),
    );
    expect(out.fetched).toBe('hello');
    expect(out.stored).toBe('hello world');
    expect(out.readOnlyRefused, 'the fetch store must refuse writes').toBe('key_not_supported');
  });
});

test.describe('STORE — the store surface, which is not blocked', () => {
  test.beforeEach(async ({ page }) => {
    await boot(page);
    await page.evaluate(
      inPage(`
        liquers.shutdown();
        await liquers.init();
        for (const k of Object.keys(localStorage)) {
          if (k.startsWith('e2e/')) localStorage.removeItem(k);
        }
        return true;
      `),
    );
  });

  /**
   * The fetch store really fetches, through the real `Store` class in a real page.
   *
   * This is what `STORE07` would assert if keyed evaluation worked: bytes from an HTTP server,
   * reached through a configuration document, in a browser. Only the last hop — the asset manager
   * — is missing.
   */
  test('a configured fetch store serves bytes over HTTP', async ({ page }) => {
    const out = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore({
          stores: [{ type: 'http', prefix: 'data',
                     config: { url_prefix: './fixtures/', keys: ['input.txt', 'sub/report.txt'] } }],
        });
        const flat = new TextDecoder().decode(await env.store().get('data/input.txt'));
        const nested = new TextDecoder().decode(await env.store().get('data/sub/report.txt'));
        const listing = await env.store().listdir('data');
        return { flat, nested, listing: listing.sort() };
      `),
    );
    expect(out.flat).toBe('hello');
    expect(out.nested, 'a nested key must map onto a nested URL').toBe('nested');
    expect(out.listing).toEqual(['input.txt', 'sub']);
  });

  /**
   * A `localStorage` value survives a page reload, index and empty directories included.
   *
   * The reload is the point: it is the only check that the directory index is rebuilt by scanning
   * storage rather than living in the first page's memory, and it needs no asset manager.
   */
  test('a localStorage value and an empty directory survive a reload', async ({ page }) => {
    const config = `{ stores: [{ type: 'localstorage', prefix: 'local',
                     config: { namespace: 'e2e' } }] }`;

    await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore(${config});
        await env.store().set('local/in.txt', new TextEncoder().encode('hello'));
        await env.store().makedir('local/empty');
        return true;
      `),
    );

    await page.reload();
    await awaitReady(page);

    const out = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore(${config});
        const text = new TextDecoder().decode(await env.store().get('local/in.txt'));
        const emptyStillThere = await env.store().isDir('local/empty');
        const listing = await env.store().listdir('local');
        return { text, emptyStillThere, listing: listing.sort() };
      `),
    );
    expect(out.text).toBe('hello');
    expect(out.emptyStillThere, 'an empty directory must survive a reload').toBe(true);
    expect(out.listing).toEqual(['empty', 'in.txt']);
  });

  /** STORE11 — one document, two stores, routed by prefix, with the read-only half refusing. */
  test('STORE11 fetch and localStorage coexist in one configuration', async ({ page }) => {
    const out = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        await env.configureStore({
          stores: [
            { type: 'http', prefix: 'data',
              config: { url_prefix: './fixtures/', keys: ['input.txt'] } },
            { type: 'localstorage', prefix: 'local', config: { namespace: 'e2e' } },
          ],
        });
        const fetched = new TextDecoder().decode(await env.store().get('data/input.txt'));
        await env.store().set('local/out.txt', new TextEncoder().encode(fetched + ' world'));
        const stored = new TextDecoder().decode(await env.store().get('local/out.txt'));

        let readOnlyRefused = null;
        try {
          await env.store().set('data/nope.txt', new TextEncoder().encode('x'));
        } catch (e) {
          readOnlyRefused = e.errorType;
        }
        return { fetched, stored, readOnlyRefused };
      `),
    );
    expect(out.fetched).toBe('hello');
    expect(out.stored).toBe('hello world');
    expect(out.readOnlyRefused, 'the fetch store must refuse writes').toBe('key_not_supported');
  });

  /** A page-implemented store is reachable through the same surface. */
  test('a JavaScript store is reachable through the store surface', async ({ page }) => {
    const out = await page.evaluate(
      inPage(`
        const env = liquers.Environment.global();
        env.registerStoreObject('mine', {
          async get(key) {
            if (key !== 'mem/greeting.txt') return undefined;
            return { data: new TextEncoder().encode('from the page'),
                     metadata: { media_type: 'text/plain' } };
          },
          async contains(key) { return key === 'mem/greeting.txt'; },
        });
        await env.configureStore({
          stores: [{ type: 'js', prefix: 'mem', config: { object: 'mine' } }],
        });
        const text = new TextDecoder().decode(await env.store().get('mem/greeting.txt'));
        const media = (await env.store().getMetadata('mem/greeting.txt')).media_type;
        return { text, media };
      `),
    );
    expect(out.text).toBe('from the page');
    expect(out.media, 'partial metadata from a page must normalize').toBe('text/plain');
  });
});
