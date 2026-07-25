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

// A static tree: one UISpecElement with two children, rendered without any interaction.
test('a UISpec root renders its static children', async ({ page }) => {
  const errors: string[] = [];
  watchErrors(page, errors);

  await page.goto('/');

  await expect(elements(page)).toHaveCount(3, { timeout: 20_000 }); // root + two children
  await expect(page.locator('#app')).toContainText('Hello from Child 1!');
  await expect(page.locator('#app')).toContainText('Hello from Child 2!');

  // The layout the spec asked for is expressed as a class, so CSS can arrange the children.
  await expect(page.locator('.lq-layout-horizontal')).toHaveCount(1);

  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});
