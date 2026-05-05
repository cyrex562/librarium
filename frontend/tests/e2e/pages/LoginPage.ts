import { Page, Locator } from '@playwright/test';

export class LoginPage {
  readonly page: Page;
  readonly usernameInput: Locator;
  readonly passwordInput: Locator;
  readonly submitButton: Locator;
  readonly errorAlert: Locator;
  readonly brandingHeading: Locator;

  constructor(page: Page) {
    this.page = page;
    this.usernameInput = page.locator('[data-testid="login-username-input"] input');
    this.passwordInput = page.locator('[data-testid="login-password-input"] input');
    this.submitButton = page.locator('[data-testid="login-submit-btn"]');
    this.errorAlert = page.locator('[data-testid="login-error-alert"]');
    this.brandingHeading = page.locator('text=Librarium').first();
  }

  async goto() {
    await this.page.goto('/');
    // App will redirect to login if not authenticated
  }

  async login(username: string, password: string) {
    await this.usernameInput.fill(username);
    await this.passwordInput.fill(password);
    const loginResponse = this.page.waitForResponse((response) =>
      response.url().includes('/api/auth/login') &&
      response.request().method() === 'POST'
    );
    await this.submitButton.click();
    await loginResponse.catch(() => null);
    await this.page.waitForFunction(
      () =>
        window.location.pathname !== '/login' ||
        !!document.querySelector('[data-testid="login-error-alert"]'),
      null,
      { timeout: 10000 }
    ).catch(() => null);
  }

  async waitForError() {
    await this.errorAlert.waitFor({ state: 'visible', timeout: 15000 });
  }

  async getErrorText(): Promise<string> {
    return await this.errorAlert.textContent() || '';
  }
}
