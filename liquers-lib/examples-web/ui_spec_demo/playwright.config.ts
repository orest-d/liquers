import { defineConfig } from '@playwright/test';

// e2e acceptance for the async-wasm-refactor: drives ui_spec_demo (compiled to wasm, backed by
// ImmediateAssetManager) in headless Chromium. `trunk serve` builds+serves the wasm bundle.
export default defineConfig({
  testDir: './tests',
  timeout: 60_000,
  use: {
    baseURL: 'http://127.0.0.1:8080',
    headless: true,
  },
  webServer: {
    command: 'trunk serve',
    url: 'http://127.0.0.1:8080',
    reuseExistingServer: true,
    timeout: 180_000,
  },
});
