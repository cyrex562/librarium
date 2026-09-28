import { test, expect, Page } from '@playwright/test';
import { LoginPage, MainLayout, VaultManager } from './pages';

// Real server: the Preview goes through the server's Typst compiler
// (POST /api/vaults/{id}/render-typst), not a mock.

async function createTestVault(page: Page) {
  const mainLayout = new MainLayout(page);
  const vaultManager = new VaultManager(page);
  const vaultName = `typst-vault-${Date.now()}`;
  await mainLayout.openVaultSettings();
  await vaultManager.waitForModal();
  await vaultManager.createVault(vaultName);
  await page.waitForTimeout(1500);
  await vaultManager.close();
  await mainLayout.selectVault(vaultName);
  await page.waitForTimeout(500);
}

test.describe('Typst notes', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await new LoginPage(page).login('admin', 'admin');
    await new MainLayout(page).waitForMainUI();
    await createTestVault(page);
  });

  test('5.1 - A new .typ note previews through the Typst compiler, errors included', async ({ page }) => {
    await page.locator('button[title="New note"]').click();
    await page.getByLabel('File name').fill('paper.typ');
    await page.locator('button:has-text("Create")').click();
    await expect(page.locator('.tab-item', { hasText: 'paper.typ' })).toBeVisible({ timeout: 5000 });

    const editor = page.locator('[data-testid="typst-editor"] .typst-editor');
    await editor.click();
    await page.keyboard.type('= Findings\n\nThe *key* result: $x^2$.\n\n#table(columns: 2, [a], [b])\n');
    await expect(editor.locator('.typ-heading')).toHaveText('= Findings');

    await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Preview' }).click();
    const body = page.locator('[data-testid="typst-preview-body"]');
    await expect(body.locator('h2')).toHaveText('Findings', { timeout: 15_000 });
    await expect(body.locator('strong')).toHaveText('key');
    await expect(body.locator('math')).toHaveCount(1);
    await expect(body.locator('td')).toHaveCount(2);
    await expect(page.locator('[data-testid="typst-diagnostics"]')).toHaveCount(0);

    // Break it: the compiler's error comes back with its line number.
    await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Formatted' }).click();
    await editor.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('#nosuchfunction()');
    await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Preview' }).click();
    const diag = page.locator('[data-testid="typst-diagnostics"]');
    await expect(diag).toContainText('nosuchfunction', { timeout: 15_000 });
    await expect(diag.getByRole('button', { name: 'Line 6' })).toBeVisible();
    // The last good render stays visible, dimmed.
    await expect(body.locator('h2')).toHaveText('Findings');
  });

  test('5.2 - Export as PDF downloads a real PDF of the current text', async ({ page }) => {
    await page.locator('button[title="New note"]').click();
    await page.getByLabel('File name').fill('report.typ');
    await page.locator('button:has-text("Create")').click();
    await expect(page.locator('.tab-item', { hasText: 'report.typ' })).toBeVisible({ timeout: 5000 });

    const editor = page.locator('[data-testid="typst-editor"] .typst-editor');
    await editor.click();
    await page.keyboard.type('= Quarterly report\n\nRevenue grew by *12%*.\n\n$ sum_(i=1)^n x_i $\n\n#pagebreak()\n= Appendix\n');

    const downloadPromise = page.waitForEvent('download');
    await page.locator('[data-testid="typst-export-pdf"]').click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe('report.pdf');
    const target = test.info().outputPath('report.pdf');
    await download.saveAs(target);
    const fs = await import('node:fs');
    const bytes = fs.readFileSync(target);
    expect(bytes.subarray(0, 5).toString()).toBe('%PDF-');
    if (process.env.TYPST_PDF_COPY) fs.copyFileSync(target, process.env.TYPST_PDF_COPY);
  });

  test('5.3 - A Markdown note converts to Typst and exports to PDF', async ({ page }) => {
    await page.locator('button[title="New note"]').click();
    await page.getByLabel('File name').fill('plan.md');
    await page.locator('button:has-text("Create")').click();
    await expect(page.locator('.tab-item', { hasText: 'plan.md' })).toBeVisible({ timeout: 5000 });

    await page.locator('.markdown-editor').click();
    // (No typed list: the editor continues "- " on Enter, which would nest it.)
    await page.keyboard.type('# Plan\n\nShip **PDF export**, see [[Roadmap]].\n\n![logo](https://example.com/logo.png)\n');

    // Markdown → PDF straight from the editor.
    await page.getByTitle('More options').click();
    const downloadPromise = page.waitForEvent('download');
    await page.locator('[data-testid="toolbar-export-pdf"]').click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe('plan.pdf');
    const target = test.info().outputPath('plan.pdf');
    await download.saveAs(target);
    const fs = await import('node:fs');
    expect(fs.readFileSync(target).subarray(0, 5).toString()).toBe('%PDF-');
    if (process.env.TYPST_PDF_COPY) fs.copyFileSync(target, process.env.TYPST_PDF_COPY);

    // Convert to Typst: the remote image can't come along, and says so.
    await page.getByTitle('More options').click();
    await page.locator('[data-testid="toolbar-convert-typst"]').click();
    const report = page.locator('[data-testid="conversion-report"]');
    await expect(report).toContainText('Created plan.typ from plan.md', { timeout: 10_000 });
    await expect(report).toContainText('Remote image');
    await report.getByRole('button', { name: 'OK' }).click();

    await expect(page.locator('.tab-item', { hasText: 'plan.typ' })).toBeVisible();
    const editor = page.locator('[data-testid="typst-editor"] .typst-editor');
    await expect(editor).toContainText('= Plan');
    await expect(editor).toContainText('#strong[PDF export]');
    await expect(editor).toContainText('#link("librarium://note/Roadmap")[Roadmap]');

    // And the converted note renders.
    await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Preview' }).click();
    await expect(page.locator('[data-testid="typst-preview-body"] h2')).toHaveText('Plan', { timeout: 15_000 });
    await expect(page.locator('[data-testid="typst-preview-body"] strong')).toHaveText('PDF export');
  });

  test('5.4 - Typst notes are found by full-text search', async ({ page }) => {
    await page.locator('button[title="New note"]').click();
    await page.getByLabel('File name').fill('field-notes.typ');
    await page.locator('button:has-text("Create")').click();
    await expect(page.locator('.tab-item', { hasText: 'field-notes.typ' })).toBeVisible({ timeout: 5000 });
    const editor = page.locator('[data-testid="typst-editor"] .typst-editor');
    // No wait for the editor to be ready: typing straight after the note
    // opens used to be lost (CodeJar was loaded asynchronously).
    await editor.click();
    await page.keyboard.type('#set page(margin: 1cm)\n= Sightings\n\nSaw a *quokkaxyz* today.\n');
    await expect(page.getByText('Saved')).toBeVisible({ timeout: 10_000 });
    // Auto-save debounce (2 s) must have fired before searching.
    await page.waitForTimeout(3000);

    await page.locator('button[title="Search (Ctrl+Shift+F)"]').click();
    await page.getByRole('textbox', { name: 'Search', exact: true }).fill('quokkaxyz');
    await page.keyboard.press('Enter');
    const modal = page.locator('.v-dialog:visible').first();
    await expect(modal.getByText('field-notes.typ').first()).toBeVisible({ timeout: 10_000 });
    // The match line is the note's text, not its markup.
    await expect(modal.getByText('Saw a quokkaxyz today.')).toBeVisible();
  });

  test('5.5 - Links from a Typst note: backlinks and following them in Preview', async ({ page }) => {
    const newNote = async (name: string) => {
      await page.locator('button[title="New note"]').click();
      await page.getByLabel('File name').fill(name);
      await page.locator('button:has-text("Create")').click();
      await expect(page.locator('.tab-item', { hasText: name })).toBeVisible({ timeout: 5000 });
    };

    await newNote('Roadmap.md');
    await newNote('links.typ');
    await page.locator('[data-testid="typst-editor"] .typst-editor').click();
    await page.keyboard.type('= Links\n\nSee #link("librarium://note/Roadmap")[the roadmap].\n');
    await page.waitForTimeout(3000); // auto-save

    // Backlink on the Markdown note, found by scanning the Typst note.
    await page.locator('.tab-item', { hasText: 'Roadmap.md' }).click();
    const backlinksHeader = page.locator('.backlinks-header');
    await backlinksHeader.click();
    await expect(page.locator('.backlinks-panel')).toContainText('links', { timeout: 10_000 });

    // Follow the link from the rendered Typst note.
    await page.locator('.tab-item', { hasText: 'Roadmap.md' }).locator('button').click(); // close it
    await page.locator('.tab-item', { hasText: 'links.typ' }).click();
    await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Preview' }).click();
    await page.locator('[data-testid="typst-preview-body"]').getByText('the roadmap').click();
    await expect(page.locator('.tab-item', { hasText: 'Roadmap.md' })).toBeVisible({ timeout: 10_000 });
  });

  test('5.6 - A Typst note keeps its frontmatter (metadata block) through edits', async ({ page }) => {
    await page.locator('button[title="New note"]').click();
    await page.getByLabel('File name').fill('tagged.typ');
    await page.locator('button:has-text("Create")').click();
    await expect(page.locator('.tab-item', { hasText: 'tagged.typ' })).toBeVisible({ timeout: 5000 });

    // Add a tag in the note header: stored in the note's metadata block.
    await page.getByTitle('Add tag').click();
    await page.getByPlaceholder('tag name…').fill('physicsxyz');
    await page.keyboard.press('Enter');
    await expect(page.locator('.v-chip', { hasText: 'physicsxyz' })).toBeVisible();

    const editor = page.locator('[data-testid="typst-editor"] .typst-editor');
    await editor.click();
    await page.keyboard.type('= Notes\n\nBody text.');
    await page.waitForTimeout(3000); // auto-save

    // Reopen: the tag comes back from the file, the text doesn't show the block.
    await page.locator('.tab-item', { hasText: 'tagged.typ' }).locator('button').click();
    await page.locator('.file-tree-node', { hasText: 'tagged.typ' }).click();
    await expect(page.locator('.v-chip', { hasText: 'physicsxyz' })).toBeVisible({ timeout: 10_000 });
    await expect(editor).toContainText('Body text.');
    await expect(editor).not.toContainText('#metadata');

    // The tag is listed vault-wide.
    await page.getByText('TAGS', { exact: true }).click();
    await expect(page.getByText('physicsxyz').first()).toBeVisible();
  });
});
