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

// The payload-carrying environment: an init query populates the tree at startup, and the menu
// action re-runs it. Commands reach AppState only through the payload, so this failing would mean
// the payload plumbing is broken in the browser.
test('an init query populates the tree and the menu action adds to it', async ({ page }) => {
  const errors: string[] = [];
  watchErrors(page, errors);

  await page.goto('/');

  await expect(page.locator('#app')).toContainText('Hello from the payload app!', {
    timeout: 20_000,
  });
  const initial = await elements(page).count();

  await page.locator('.lq-menubar').getByText('Re-evaluate hello').click();
  await expect(elements(page)).toHaveCount(initial + 1, { timeout: 20_000 });

  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});
