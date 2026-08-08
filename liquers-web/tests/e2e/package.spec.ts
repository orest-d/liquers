import { test, expect } from '@playwright/test';
import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

/**
 * Browser-tier conformance tests: `PACKAGE02`, `PACKAGE03`, `PACKAGE07`, `STUBS07`.
 *
 * These four need a real page. The rest of `PACKAGE`/`STUBS` are structural checks on generated
 * text and run in `check-stubs.sh`, which needs no browser.
 *
 * The page under test is `examples-web/quickstart`, built by its `build.sh`. Run that first.
 */

const DIST = resolve(__dirname, '../../examples-web/quickstart/dist');

test.describe('PACKAGE — the delivery form works', () => {
  /**
   * PACKAGE02 — the artifact loads in a clean environment with no bundler.
   *
   * A fresh browser context, a plain `<script type="module">`, an ES module served over HTTP.
   * Nothing is installed and nothing is bundled — which is the delivery form Phase 1 decision 7
   * chose, and the only way to check it is to actually do it.
   */
  test('PACKAGE02 install into clean environment loads', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();

    const failures: string[] = [];
    page.on('pageerror', (e) => failures.push(`pageerror: ${e.message}`));
    page.on('requestfailed', (r) =>
      failures.push(`requestfailed: ${r.url()} ${r.failure()?.errorText}`),
    );

    await page.goto('/index.html');
    await page.waitForFunction(() => (globalThis as any).__liquersQuickstart?.ready === true);

    expect(failures, 'loading the module must produce no errors').toEqual([]);
    await context.close();
  });

  /**
   * PACKAGE03 — the quick start evaluates queries end to end, with **zero console errors**.
   *
   * The milestone gate. "Zero console errors" is asserted, not assumed: a wasm module that
   * initializes and then throws inside a Promise leaves a working-looking page and a console full
   * of unhandled rejections.
   */
  test('PACKAGE03 quickstart evaluates a query end to end', async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on('console', (msg) => {
      if (msg.type() === 'error') consoleErrors.push(msg.text());
    });
    page.on('pageerror', (e) => consoleErrors.push(`pageerror: ${e.message}`));

    await page.goto('/index.html');
    await page.waitForFunction(() => (globalThis as any).__liquersQuickstart?.ready === true);

    // Every query the page runs, checked by its rendered result rather than by an internal flag.
    const rows = await page.$$eval('#results tr', (trs) =>
      trs.map((tr) => ({
        query: tr.children[0].textContent?.trim() ?? '',
        result: tr.children[1].textContent?.trim() ?? '',
        failed: tr.children[1].classList.contains('err'),
      })),
    );

    const byQuery = new Map(rows.map((r) => [r.query, r]));

    expect(byQuery.get('hello')?.result).toBe('"Hello, world!"');
    expect(byQuery.get('hello/shout')?.result).toBe('"HELLO, WORLD!"');
    expect(byQuery.get('hello/repeat-3')?.result).toBe(
      '"Hello, world!Hello, world!Hello, world!"',
    );
    // The declared default binds when the query omits the argument.
    expect(byQuery.get('hello/repeat')?.result).toBe('"Hello, world!Hello, world!"');
    // A built-in Rust command reading a JavaScript command's result — the composition that
    // justifies structural conversion being the default.
    expect(byQuery.get('hello/to_text')?.result).toBe('"Hello, world!"');
    expect(byQuery.get('hello/repeat-2/shout')?.result).toBe(
      '"HELLO, WORLD!HELLO, WORLD!"',
    );
    // An async command resolves.
    expect(byQuery.get('delayed')?.result).toBe('"after a tick"');
    // `encodeParam` produced the escaped form.
    expect(byQuery.get('encodeParam("two words")')?.result).toBe('two~.words');

    // The deliberate failure is a *structured* error, showing its type — not a bare string.
    const bad = byQuery.get('hello/no_such_command');
    expect(bad?.failed, 'the unknown command must be reported as a failure').toBe(true);
    expect(bad?.result).toContain('action_not_registered');

    const state = await page.evaluate(() => (globalThis as any).__liquersQuickstart);
    expect(state.failures, 'no query should fail unexpectedly').toBe(0);

    await expect(page.locator('#status')).toContainText('Ready');

    expect(consoleErrors, 'the quick start must produce zero console errors').toEqual([]);
  });

  /**
   * PACKAGE04 — `version()` reports the crate version and the version of the *linked* core.
   *
   * Asserted by calling it, not by grepping the artifact for a version string. Both would pass
   * today; only this one fails if `version()` starts returning a hard-coded literal that has
   * drifted from the manifests — which is the failure the ID exists to catch.
   */
  test('PACKAGE04 version metadata matches linked core', async ({ page }) => {
    const root = resolve(__dirname, '../../..');
    const versionOf = (manifest: string): string => {
      const text = readFileSync(resolve(root, manifest), 'utf8');
      const match = text.match(/^version\s*=\s*"([^"]+)"/m);
      expect(match, `no version in ${manifest}`).not.toBeNull();
      return match![1];
    };
    const webVersion = versionOf('liquers-web/Cargo.toml');
    const coreVersion = versionOf('liquers-core/Cargo.toml');

    await page.goto('/index.html');
    await page.waitForFunction(() => (globalThis as any).__liquersQuickstart?.ready === true);

    const reported: string = await page.evaluate(async () => {
      const mod: any = await import('./liquers_web.js');
      return mod.version();
    });

    expect(reported).toBe(`liquers-web ${webVersion} (liquers-core ${coreVersion})`);
  });

  /**
   * PACKAGE07 — the artifact ships its declarations, license and metadata beside the wasm.
   */
  test('PACKAGE07 artifact carries declarations license and metadata', async () => {
    // Generated declarations sit next to the module a page imports.
    expect(existsSync(resolve(DIST, 'liquers_web.d.ts'))).toBe(true);
    expect(existsSync(resolve(DIST, 'liquers_web_bg.wasm.d.ts'))).toBe(true);
    expect(existsSync(resolve(DIST, 'liquers_web.js'))).toBe(true);
    expect(existsSync(resolve(DIST, 'liquers_web_bg.wasm'))).toBe(true);

    // The repository's license applies to the artifact; assert one exists to ship.
    const root = resolve(__dirname, '../../..');
    const licensed =
      existsSync(resolve(root, 'LICENSE')) ||
      existsSync(resolve(root, 'LICENSE.txt')) ||
      existsSync(resolve(root, 'LICENSE-MIT')) ||
      existsSync(resolve(root, 'LICENSE-APACHE'));
    expect(licensed, 'a LICENSE must exist to ship alongside the artifact').toBe(true);

    // Metadata: the crate's own version, reachable from the page rather than only from Cargo.toml.
    const manifest = readFileSync(resolve(root, 'liquers-web/Cargo.toml'), 'utf8');
    expect(manifest).toMatch(/^version\s*=\s*"/m);
    expect(manifest).toMatch(/^description\s*=\s*"/m);
  });
});

test.describe('STUBS — the declarations ship and describe the real surface', () => {
  /**
   * STUBS07 — the declarations ship in the artifact and match what the page actually exposes.
   *
   * The browser half: every function the `.d.ts` declares must exist on the loaded module. A
   * declaration file that has drifted from the build is worse than none, because a type checker
   * will confidently accept code that fails at runtime.
   */
  test('STUBS07 declarations ship in the artifact', async ({ page }) => {
    const declarations = readFileSync(resolve(DIST, 'liquers_web.d.ts'), 'utf8');
    expect(declarations.length).toBeGreaterThan(0);

    const declaredFunctions = [
      ...declarations.matchAll(/^export function (\w+)\(/gm),
    ].map((m) => m[1]);
    const declaredClasses = [...declarations.matchAll(/^export class (\w+)/gm)].map(
      (m) => m[1],
    );

    expect(declaredFunctions.length, 'the .d.ts must declare functions').toBeGreaterThan(5);
    expect(declaredClasses).toEqual(
      expect.arrayContaining(['Query', 'Key', 'Value', 'Asset', 'State', 'Environment']),
    );

    await page.goto('/index.html');
    await page.waitForFunction(() => (globalThis as any).__liquersQuickstart?.ready === true);

    // Load the module a second time in page context and check every declared name is present.
    const missing = await page.evaluate(
      async ([fns, classes]: [string[], string[]]) => {
        const mod: Record<string, unknown> = await import('./liquers_web.js');
        const absent: string[] = [];
        for (const name of fns) {
          if (typeof mod[name] !== 'function') absent.push(`function ${name}`);
        }
        for (const name of classes) {
          if (typeof mod[name] !== 'function') absent.push(`class ${name}`);
        }
        return absent;
      },
      [declaredFunctions, declaredClasses] as [string[], string[]],
    );

    expect(missing, 'every declared name must exist on the loaded module').toEqual([]);
  });
});
