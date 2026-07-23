import { test, expect } from '@playwright/test';

// The acceptance test for the async-wasm-refactor: the wasm app must RUN in a real browser,
// exercising mount_web -> evaluate_immediately -> inline ImmediateAssetManager eval -> re-render,
// with no `tokio::spawn` "no reactor" panic and no Send error (both would surface as pageerror).
test('dashboard renders and reacts to a menu action', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e}`));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push(`console.error: ${m.text()}`);
  });

  await page.goto('/');

  // 1. Initial render: the menu button declared in DASHBOARD_YAML is produced by the webui
  //    backend after mount_web's first evaluation.
  await expect(page.locator('#app')).toContainText('Add Dashboard', { timeout: 20_000 });

  // 2. Drive the menu action: click -> UiAction dispatch -> evaluate_immediately -> inline eval
  //    (ImmediateAssetManager, no spawn) -> ui::web re-render.
  await page.getByText('Add Dashboard').click();

  // 3. A dashboard child appears — proves the whole inline evaluation loop ran in the browser.
  await expect(page.locator('#app')).toContainText('Dashboard', { timeout: 20_000 });

  // 4. No uncaught panic/error. A reintroduced tokio::spawn on the inline path would panic here.
  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});
