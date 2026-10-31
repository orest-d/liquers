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

// The query console in the browser (specs/design/ui-events/): Enter and Go both send a `Submit`
// UI event carrying the typed query to the console, which owns it from then on; clicking into the
// input is not an activation; the menu's `Alt+N` shortcut runs its item.
const openConsole = async (page: any) => {
  await page.goto('/');
  await page.locator('.lq-menubar').getByText('New Console').click();
  await expect(page.locator('.lq-QueryConsoleElement')).toHaveCount(1, { timeout: 20_000 });
  await expect(page.locator('.lq-qc-content')).toContainText('Hello from the query console!', {
    timeout: 20_000,
  });
};

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

test('clicking Go submits the typed query and the console keeps it', async ({ page }) => {
  const errors: string[] = [];
  watchErrors(page, errors);
  await openConsole(page);

  await page.locator('.lq-query-input').fill('about');
  await page.locator('.lq-go').click();

  await expect(page.locator('.lq-qc-content')).toContainText(
    'Liquers query console — browser port',
    { timeout: 20_000 },
  );
  // W2: the re-rendered input shows the submitted query, not the one the console started with.
  await expect(page.locator('.lq-query-input')).toHaveValue('about');
  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});

test('pressing Enter in the input submits it', async ({ page }) => {
  const errors: string[] = [];
  watchErrors(page, errors);
  await openConsole(page);

  await page.locator('.lq-query-input').fill('about');
  await page.locator('.lq-query-input').press('Enter');

  await expect(page.locator('.lq-qc-content')).toContainText(
    'Liquers query console — browser port',
    { timeout: 20_000 },
  );
  await expect(page.locator('.lq-query-input')).toHaveValue('about');
  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});

test('clicking into the input does not submit', async ({ page }) => {
  await openConsole(page);

  await page.locator('.lq-query-input').fill('about');
  await page.locator('.lq-query-input').click();
  await page.waitForTimeout(1000);

  await expect(page.locator('.lq-qc-content')).toContainText('Hello from the query console!');
  await expect(page.locator('.lq-query-input')).toHaveValue('about');
});

test('the menu shortcut adds a console, also from inside the input', async ({ page }) => {
  const errors: string[] = [];
  watchErrors(page, errors);
  await openConsole(page);

  // Focus on nothing in particular: the key targets <body>, outside the mount root.
  await page.locator('body').click({ position: { x: 1, y: 1 } });
  await page.keyboard.press('Alt+N');
  await expect(page.locator('.lq-QueryConsoleElement')).toHaveCount(2, { timeout: 20_000 });

  await page.locator('.lq-query-input').first().press('Alt+N');
  await expect(page.locator('.lq-QueryConsoleElement')).toHaveCount(3, { timeout: 20_000 });
  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});

test('typing a shortcut letter in the input only edits the text', async ({ page }) => {
  await openConsole(page);

  await page.locator('.lq-query-input').fill('');
  await page.locator('.lq-query-input').pressSequentially('n');
  await page.waitForTimeout(1000);

  await expect(page.locator('.lq-QueryConsoleElement')).toHaveCount(1);
  await expect(page.locator('.lq-query-input')).toHaveValue('n');
});
