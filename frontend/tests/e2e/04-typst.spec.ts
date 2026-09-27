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
});
