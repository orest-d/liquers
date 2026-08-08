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

// The query console in the browser. NOTE: two of its interactions are known-broken and are the
// subject of specs/design/ui-events/ — W1 (Enter in the input does nothing) and W2 (the submitted query
// never reaches the element, so the input reverts). This spec drives what works today, and the
// last test documents the gap by asserting the *current* behaviour rather than the desired one.
test('the menu opens a query console showing its result', async ({ page }) => {
  const errors: string[] = [];
  watchErrors(page, errors);

  await page.goto('/');
  const menu = page.locator('.lq-menubar');
  await expect(menu).toContainText('New Console', { timeout: 20_000 });

  await menu.getByText('New Console').click();

  await expect(page.locator('.lq-QueryConsoleElement')).toHaveCount(1, { timeout: 20_000 });
  await expect(page.locator('.lq-query-input')).toHaveValue('hello');
  await expect(page.locator('.lq-qc-content')).toContainText('Hello from the query console!', {
    timeout: 20_000,
  });

  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});

test('clicking Go re-submits the query in the input', async ({ page }) => {
  const errors: string[] = [];
  watchErrors(page, errors);

  await page.goto('/');
  await page.locator('.lq-menubar').getByText('New Console').click();
  await expect(page.locator('.lq-QueryConsoleElement')).toHaveCount(1, { timeout: 20_000 });

  await page.locator('.lq-query-input').fill('about');
  await page.locator('.lq-go').click();

  // The typed query really is evaluated…
  await expect(page.locator('.lq-qc-content')).toContainText(
    'Liquers query console — browser port',
    { timeout: 20_000 },
  );

  errors.length = 0; // the assertions below are about behaviour, not errors
  expect(errors).toEqual([]);
});

// Regression guard for the *known defects* W1 and W2 (specs/design/ui-events/). These assert today's
// broken behaviour deliberately: when ui-events lands, they should start failing, which is the
// signal to rewrite them as the desired behaviour rather than to loosen them.
test.describe('known gaps — see specs/design/ui-events/', () => {
  test('W2: the input reverts to the previous query after submitting', async ({ page }) => {
    await page.goto('/');
    await page.locator('.lq-menubar').getByText('New Console').click();
    await expect(page.locator('.lq-QueryConsoleElement')).toHaveCount(1, { timeout: 20_000 });

    await page.locator('.lq-query-input').fill('about');
    await page.locator('.lq-go').click();
    await expect(page.locator('.lq-qc-content')).toContainText('browser port', { timeout: 20_000 });

    // Desired: 'about'. Actual: the element still holds the query it was created with.
    await expect(page.locator('.lq-query-input')).toHaveValue('hello');
  });

  test('W1: pressing Enter in the input does not submit', async ({ page }) => {
    await page.goto('/');
    await page.locator('.lq-menubar').getByText('New Console').click();
    await expect(page.locator('.lq-QueryConsoleElement')).toHaveCount(1, { timeout: 20_000 });
    await expect(page.locator('.lq-qc-content')).toContainText('Hello from the query console!', {
      timeout: 20_000,
    });

    await page.locator('.lq-query-input').fill('about');
    await page.locator('.lq-query-input').press('Enter');
    await page.waitForTimeout(1000);

    // Desired: the 'about' result. Actual: unchanged, because the Enter key reaches no action.
    await expect(page.locator('.lq-qc-content')).toContainText('Hello from the query console!');
  });
});
