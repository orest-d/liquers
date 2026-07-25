import { test, expect } from '@playwright/test';

// Shared helpers: every webui element renders into a node with a stable `ui-element-{handle}` id,
// so counting them counts the rendered tree, and a browser error is always a failure.
const watchErrors = (page: any, errors: string[]) => {
  page.on('pageerror', (e: any) => errors.push(`pageerror: ${e}`));
  page.on('console', (m: any) => {
    if (m.type() === 'error') errors.push(`console.error: ${m.text()}`);
  });
};
const elements = (page: any) => page.locator('[id^="ui-element-"]');

// Menu actions built in Rust (not YAML) drive the same UiAction dispatch as every other backend.
test('menu actions add and remove children', async ({ page }) => {
  const errors: string[] = [];
  watchErrors(page, errors);

  await page.goto('/');
  const menu = page.locator('.lq-menubar');
  await expect(menu).toContainText('Add Hello', { timeout: 20_000 });

  const initial = await elements(page).count();

  await menu.getByText('Add Hello').click();
  await expect(elements(page)).toHaveCount(initial + 1, { timeout: 20_000 });
  await expect(page.locator('#app')).toContainText('Hello from menu button!');

  await menu.getByText('Add Hello').click();
  await expect(elements(page)).toHaveCount(initial + 2, { timeout: 20_000 });

  // "Clear All" removes the last child; it resolves inline, so nothing async nudges the renderer.
  await menu.getByText('Clear All').click();
  await expect(elements(page)).toHaveCount(initial + 1, { timeout: 5_000 });

  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});
