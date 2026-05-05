import { Page, Locator } from '@playwright/test';

export class MainLayout {
  readonly page: Page;
  readonly vaultSelector: Locator;
  readonly vaultSettingsBtn: Locator;
  readonly userMenuBtn: Locator;
  readonly userMenuSignOut: Locator;
  readonly userMenuChangePassword: Locator;
  readonly userMenuManageUsers: Locator;
  readonly brandingText: Locator;

  constructor(page: Page) {
    this.page = page;
    this.vaultSelector = page.locator('[data-testid="vault-selector"]');
    this.vaultSettingsBtn = page.locator('[data-testid="vault-settings-btn"]');
    this.userMenuBtn = page.locator('[data-testid="topbar-user-menu-btn"]');
    this.userMenuSignOut = page.locator('[data-testid="user-menu-sign-out"]');
    this.userMenuChangePassword = page.locator('[data-testid="user-menu-change-password"]');
    this.userMenuManageUsers = page.locator('[data-testid="user-menu-manage-users"]');
    this.brandingText = page.locator('text=Librarium');
  }

  async openUserMenu() {
    await this.userMenuBtn.click();
    await this.page.waitForTimeout(500); // Wait for menu animation
  }

  async logout() {
    await this.openUserMenu();
    await this.userMenuSignOut.waitFor({ state: 'visible', timeout: 3000 });
    await this.userMenuSignOut.click();
  }

  async goToChangePassword() {
    await this.openUserMenu();
    await this.userMenuChangePassword.waitFor({ state: 'visible', timeout: 3000 });
    await this.userMenuChangePassword.click();
  }

  async goToManageUsers() {
    await this.openUserMenu();
    await this.userMenuManageUsers.waitFor({ state: 'visible', timeout: 3000 });
    await this.userMenuManageUsers.click();
  }

  async openVaultSettings() {
    await this.vaultSettingsBtn.click();
  }

  async selectVault(vaultName: string) {
    await this.vaultSelector.click();
    await this.page.locator(`text=${vaultName}`).click();
  }

  async waitForMainUI() {
    await Promise.race([
      this.vaultSettingsBtn.waitFor({ state: 'visible', timeout: 10000 }),
      this.userMenuBtn.waitFor({ state: 'visible', timeout: 10000 }),
      this.page.getByText('Select a vault to start.').waitFor({ state: 'visible', timeout: 10000 }),
    ]);
  }
}
