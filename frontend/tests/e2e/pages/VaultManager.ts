import { Page, Locator } from '@playwright/test';

export class VaultManager {
  readonly page: Page;
  readonly modal: Locator;
  readonly closeButton: Locator;
  readonly vaultNameInput: Locator;
  readonly vaultPathInput: Locator;
  readonly createButton: Locator;
  readonly errorAlert: Locator;

  constructor(page: Page) {
    this.page = page;
    this.modal = page.locator('.v-dialog');
    this.closeButton = page.locator('[data-testid="vault-manager-close-btn"]');
    this.vaultNameInput = page.locator('[data-testid="vault-name-input"] input');
    this.vaultPathInput = page.locator('[data-testid="vault-path-input"] input');
    this.createButton = page.locator('[data-testid="add-vault-btn"]');
    this.errorAlert = page.locator('[data-testid="vault-error-alert"]');
  }

  async waitForModal() {
    await this.modal.waitFor({ state: 'visible', timeout: 5000 });
  }

  async createVault(name: string, path?: string) {
    await this.vaultNameInput.fill(name);
    if (path) {
      await this.vaultPathInput.fill(path);
    }
    await this.createButton.click();
  }

  async close() {
    await this.closeButton.click();
  }

  async getErrorText(): Promise<string> {
    return await this.errorAlert.textContent() || '';
  }

  async waitForError() {
    await this.errorAlert.waitFor({ state: 'visible', timeout: 5000 });
  }
}
