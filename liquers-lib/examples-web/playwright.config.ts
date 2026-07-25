import { defineConfig } from '@playwright/test';

// One Playwright harness for every browser example. Each example is a Playwright *project* with
// its own dev server on its own port and a spec file named after it, so `page.goto('/')` in a spec
// lands on that example.
//
// Ports are declared here and in each example's Trunk.toml, so `trunk serve` run by hand in an
// example directory serves on the same port its test expects.
export const examples = [
  { name: 'ui_spec_demo', port: 8080 },
  { name: 'ui_hello', port: 8081 },
  { name: 'ui_spec_simple', port: 8082 },
  { name: 'ui_spec_interactive', port: 8083 },
  { name: 'ui_payload_app', port: 8084 },
  { name: 'ui_button_app', port: 8085 },
  { name: 'ui_query_console_app', port: 8086 },
];

export default defineConfig({
  testDir: './tests',
  timeout: 60_000,
  use: {
    headless: true,
  },
  projects: examples.map((e) => ({
    name: e.name,
    testMatch: `${e.name}.spec.ts`,
    use: { baseURL: `http://127.0.0.1:${e.port}` },
  })),
  // The first run compiles every example to wasm. They share one target directory, so cargo
  // serialises the builds — hence the generous timeout.
  webServer: examples.map((e) => ({
    command: 'trunk serve',
    cwd: e.name,
    url: `http://127.0.0.1:${e.port}`,
    reuseExistingServer: true,
    timeout: 600_000,
  })),
});
