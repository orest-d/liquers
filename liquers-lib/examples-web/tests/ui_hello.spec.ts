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

// The pending root is evaluated in the browser and replaced by an element holding the result.
test('a pending root evaluates and renders its value', async ({ page }) => {
  const errors: string[] = [];
  watchErrors(page, errors);

  await page.goto('/');

  await expect(page.locator('#app')).toContainText('Hello, World!', { timeout: 20_000 });
  await expect(elements(page)).toHaveCount(1);

  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});
