import { Page, Locator } from '@playwright/test';

export class FileTree {
  readonly page: Page;

  constructor(page: Page) {
    this.page = page;
  }

  getFileNode(fileName: string): Locator {
    // Scoped to .file-tree-node, not a bare text= match — an open tab's
    // title can contain the same substring (e.g. "to-delete.md") and sits
    // earlier/later in the DOM depending on layout, so an unscoped match
    // can silently right-click the tab instead of the tree row.
    return this.page.locator('.file-tree-node', { hasText: fileName }).first();
  }

  async rightClickFile(fileName: string) {
    await this.getFileNode(fileName).click({ button: 'right' });
    await this.page.waitForTimeout(300); // Wait for context menu
  }

  async openContextMenuItem(itemTestId: string) {
    await this.page.locator(`[data-testid="${itemTestId}"]`).click();
  }

  async clickNewFile() {
    await this.openContextMenuItem('ctx-new-file');
  }

  async clickNewFolder() {
    await this.openContextMenuItem('ctx-new-folder');
  }

  async clickDelete() {
    await this.openContextMenuItem('ctx-delete');
  }

  async clickRename() {
    await this.openContextMenuItem('ctx-rename');
  }

  async openFile(fileName: string) {
    await this.getFileNode(fileName).click();
  }

  async createFileInFolder(folderName: string, fileName: string) {
    await this.rightClickFile(folderName);
    await this.clickNewFile();
    // Wait for prompt dialog
    await this.page.waitForTimeout(500);
    // Type filename and press enter
    await this.page.keyboard.type(fileName);
    await this.page.keyboard.press('Enter');
  }

  async deleteFile(fileName: string) {
    await this.rightClickFile(fileName);
    await this.clickDelete();
  }
}
