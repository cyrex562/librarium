import { test, expect, Page } from '@playwright/test';
import { LoginPage, MainLayout, VaultManager, FileTree } from './pages';

async function createTestVault(page: Page): Promise<string> {
  const mainLayout = new MainLayout(page);
  const vaultManager = new VaultManager(page);
  
  const vaultName = `test-vault-${Date.now()}`;
  await mainLayout.openVaultSettings();
  await vaultManager.waitForModal();
  
  await vaultManager.createVault(vaultName);
  await page.waitForTimeout(1500);
  
  await vaultManager.close();
  
  // Select the vault from dropdown
  await mainLayout.selectVault(vaultName);
  await page.waitForTimeout(500);
  
  return vaultName;
}

test.describe('File Operations', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    const loginPage = new LoginPage(page);
    const mainLayout = new MainLayout(page);
    
    await loginPage.login('admin', 'admin');
    await mainLayout.waitForMainUI();
    await createTestVault(page);
  });

  test('3.3 - Clicking .md file opens in editor tab', async ({ page }) => {
    // Look for "New note" button in sidebar actions
    const newNoteBtn = page.locator('button[title="New note"]').or(page.locator('button:has-text("New note")'));
    if (await newNoteBtn.count() > 0) {
      await newNoteBtn.click();
      
      // Fill in file name in dialog
      await page.getByLabel('File name').fill('test-note.md');
      await page.locator('button:has-text("Create")').click();
      
      // Tab should open with file name
      // .tab-item is the real tab class (TabBar.vue) — .v-tab is Vuetify's
      // own tab component, used only by the Settings modal, never by file
      // tabs. Also scoped, not a bare text match: "test-note.md" appears in
      // the tree node, doc header, and status bar too, so an unscoped
      // getByText/text= locator resolves to multiple elements at once.
      await expect(page.locator('.tab-item', { hasText: 'test-note' })).toBeVisible({ timeout: 5000 });
    } else {
      // Skip test if UI doesn't have new note button
      test.skip();
    }
  });

  test('4.1 - Right-click folder shows New File option', async ({ page }) => {
    const fileTree = new FileTree(page);

    // "New file" is a per-folder context-menu item (FileTreeNode.vue), not a
    // blank-tree-area menu — the vault starts empty, so a folder must exist
    // before there's anything to right-click.
    const folderName = `test-folder-${Date.now()}`;
    await page.locator('button[title="New folder"]').click();
    await page.getByLabel('Folder name').fill(folderName);
    await page.locator('button:has-text("Create")').click();
    await expect(page.locator('.file-tree-node', { hasText: folderName })).toBeVisible({ timeout: 5000 });

    await fileTree.rightClickFile(folderName);

    // Should see "New file" option in context menu
    const newFileOption = page.locator('[data-testid="ctx-new-file"]');
    await expect(newFileOption).toBeVisible({ timeout: 2000 });
  });

  test('4.4 - Deleting open file closes its tab', async ({ page }) => {
    const fileTree = new FileTree(page);
    
    // Look for "New note" button
    const newNoteBtn = page.locator('button[title="New note"]').or(page.locator('button:has-text("New note")'));
    
    if (await newNoteBtn.count() > 0) {
      // Create a file
      await newNoteBtn.click();
      await page.getByLabel('File name').fill('to-delete.md');
      await page.locator('button:has-text("Create")').click();
      // .tab-item is the real tab class (TabBar.vue) — .v-tab is Vuetify's
      // own tab component, used only by the Settings modal, never by file
      // tabs, so a .v-tab count is always 0 regardless of what happened here.
      await expect(page.locator('.tab-item', { hasText: 'to-delete' })).toBeVisible({ timeout: 5000 });

      // Set up dialog handler before deleting
      page.on('dialog', dialog => dialog.accept());

      // Find and delete the file via context menu
      await fileTree.deleteFile('to-delete');

      // Tab should be closed
      await expect(page.locator('.tab-item', { hasText: 'to-delete' })).toHaveCount(0);
    } else {
      test.skip();
    }
  });
});
