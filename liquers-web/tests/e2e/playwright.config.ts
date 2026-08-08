import { defineConfig } from '@playwright/test';

/**
 * Browser tests for `liquers-web`.
 *
 * These are the tests that genuinely need a browser rather than Node: `PACKAGE*` asserts that the
 * delivery form works — a plain page, no bundler, a real module import over HTTP — and that is not
 * observable from `wasm-bindgen-test`, which loads the module directly.
 *
 * The page is served as static files rather than by `trunk serve`. `build.sh` produces the same
 * `dist/` trunk would, and a static server is one less thing that has to be installed for the
 * tests to run. See `liquers-web/examples-web/quickstart/build.sh`.
 */
export default defineConfig({
  testDir: '.',
  timeout: 60_000,
  use: {
    headless: true,
    baseURL: 'http://127.0.0.1:8090',
  },
  webServer: {
    command: 'python3 -m http.server 8090 --directory ../../examples-web/quickstart/dist',
    url: 'http://127.0.0.1:8090/index.html',
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
