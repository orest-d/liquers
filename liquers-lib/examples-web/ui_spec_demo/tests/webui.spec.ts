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
  //    Counted, not matched by text: "Dashboard" is a substring of the "Add Dashboard" menu label,
  //    so a text assertion alone is satisfied before the click and proves nothing.
  await expect(page.locator('[id^="ui-element-"]')).toHaveCount(2, { timeout: 20_000 });
  await expect(page.locator('#app')).toContainText('Dashboard', { timeout: 20_000 });

  // 4. No uncaught panic/error. A reintroduced tokio::spawn on the inline path would panic here.
  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});

// Stage 2 of specs/webui-fixes/: an insert must add one node, not regenerate the parent's subtree.
// A tag set on a live node from the page survives only if that node is never replaced — which is
// the mechanism that protects focus, caret, scroll and any node-local state in siblings.
test('adding a panel preserves existing nodes', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e}`));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push(`console.error: ${m.text()}`);
  });

  await page.goto('/');
  const menu = page.locator('.lq-menubar');
  await expect(menu).toContainText('Add Dashboard', { timeout: 20_000 });

  const elements = page.locator('[id^="ui-element-"]');
  const initial = await elements.count();

  await menu.getByText('Add Dashboard').click();
  await expect(elements).toHaveCount(initial + 1, { timeout: 20_000 });

  // Tag the panel that exists now. Nothing in the model knows about this attribute, so it can
  // only survive by the node itself surviving.
  const panel = elements.nth(initial);
  await panel.evaluate((el) => el.setAttribute('data-probe', 'kept'));

  await menu.getByText('Add Dashboard').click();
  await expect(elements).toHaveCount(initial + 2, { timeout: 20_000 });

  await expect(page.locator('[data-probe="kept"]')).toHaveCount(1);

  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});

// One batch can contain both an ancestor and its descendant: *Add Panel Group* inserts a
// UISpecElement whose own init query adds a child, all before the next render tick. Rendering the
// ancestor draws its whole subtree from the final model, so the descendant already exists by the
// time its own record is applied — inserting it again would duplicate the DOM id and leave a ghost
// node behind. (Reported by Codex review on PR #12.)
test('an ancestor and its descendant in one batch do not duplicate nodes', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e}`));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push(`console.error: ${m.text()}`);
  });

  await page.goto('/');
  const menu = page.locator('.lq-menubar');
  await expect(menu).toContainText('Add Panel Group', { timeout: 20_000 });

  await menu.getByText('Add Panel Group').click();

  // Wait for the group and its init-created child to land. Deliberately `>=`: the defect this
  // guards produces *more* nodes than expected, and the assertion below is what must catch that,
  // not the arrival condition.
  const elementIds = () =>
    page.evaluate(() => Array.from(document.querySelectorAll('[id^="ui-element-"]')).map((e) => e.id));
  await expect.poll(async () => (await elementIds()).length, { timeout: 20_000 }).toBeGreaterThanOrEqual(3);

  // Each element exactly once. Duplicate ids are invalid HTML, and a later removal deletes only
  // the first match, leaving a ghost node behind. Without the already-materialized guard in
  // `insert_element_node` this reports the child twice.
  const ids = await elementIds();
  const duplicates = ids.filter((id, i) => ids.indexOf(id) !== i);
  expect(duplicates, `duplicated element ids in [${ids.join(', ')}]`).toEqual([]);

  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});

// W3 (specs/webui-fixes/): an action that resolves fully inline leaves nothing in flight, so a
// renderer that repaints on "is async work pending?" never learns about it. `Remove Last Panel`
// is exactly that case; `Add Dashboard` is not, because it leaves a pending node behind.
test('an inline action updates the DOM', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e}`));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push(`console.error: ${m.text()}`);
  });

  await page.goto('/');
  const menu = page.locator('.lq-menubar');
  await expect(menu).toContainText('Add Dashboard', { timeout: 20_000 });

  // Every element renders into a node with a stable id, so counting them counts the tree.
  // (Menu labels also appear inside the panels' text, so locators are scoped to the menu bar.)
  const elements = page.locator('[id^="ui-element-"]');
  const initial = await elements.count();

  await menu.getByText('Add Dashboard').click();
  await expect(elements).toHaveCount(initial + 1, { timeout: 20_000 });
  await menu.getByText('Add Dashboard').click();
  await expect(elements).toHaveCount(initial + 2, { timeout: 20_000 });

  // The panel must disappear on its own, with no further interaction to nudge the renderer.
  await menu.getByText('Remove Last Panel').click();
  await expect(elements).toHaveCount(initial + 1, { timeout: 5_000 });

  expect(errors, `browser errors:\n${errors.join('\n')}`).toEqual([]);
});
