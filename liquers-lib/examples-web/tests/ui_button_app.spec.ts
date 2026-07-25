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

// A custom UIElement defined in the example crate, rendering itself to HTML and replacing itself
// with its query result via `add-instead`.
test('a custom button element replaces itself with its query result', async ({ page }) => {
  const errors: string[] = [];
  watchErrors(page, errors);

  await page.goto('/');
  const button = page.locator('.lq-ButtonElement button');
  await expect(button).toHaveText('Say Hello', { timeout: 20_000 });

  await button.click();

  // `add-instead` replaces the element in place: the value appears and the button is gone.
  await expect(page.locator('#app')).toContainText('Hello, world!', { timeout: 20_000 });
  await expect(page.locator('.lq-ButtonElement')).toHaveCount(0);
  await expect(elements(page)).toHaveCount(1); // same handle, different element

  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});
