import { test, expect } from '@playwright/test';

/**
 * `UIUSE` in the delivered form: `examples-web/quickstart/ui.html` imports the built module and
 * mounts the query console with `mountUI`. Run `build.sh` first.
 */

test('mounts the console and shows a query', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/ui.html');
  await page.waitForFunction(() => (globalThis as any).__liquersUI?.ready === true);

  // The console is the first element.
  await expect(page.locator('#app [id^="ui-element-"]').first()).toBeVisible();

  // `show` adds a root whose value is the query's result.
  const handle = await page.evaluate(() => (globalThis as any).__liquersUI.ui.show('commands_doc'));
  expect(typeof handle).toBe('number');
  await expect(page.locator(`#ui-element-${handle}`)).toBeVisible();
  await expect(page.locator('#events')).toContainText(`${handle} ready`);

  // Disposing empties the container.
  await page.evaluate(() => (globalThis as any).__liquersUI.ui.dispose());
  await expect(page.locator('#app')).toBeEmpty();
  expect(errors).toEqual([]);
});
