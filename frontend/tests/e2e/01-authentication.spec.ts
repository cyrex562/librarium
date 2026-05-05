import { test, expect } from '@playwright/test';
import { LoginPage, MainLayout } from './pages';

test.describe('Authentication', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    // Vue router will redirect to /login if not authenticated
  });

  test('1.1 - App shows "Librarium" branding on login page', async ({ page }) => {
    const loginPage = new LoginPage(page);
    await expect(loginPage.brandingHeading).toBeVisible();
    await expect(page.locator('text=Obsidian Host')).not.toBeVisible();
  });

  test('1.2 - Login with wrong password shows error', async ({ page }) => {
    const loginPage = new LoginPage(page);
    await loginPage.login('admin', 'wrongpassword');
    await loginPage.waitForError();
    await expect(loginPage.errorAlert).toContainText(/invalid|incorrect|failed/i);
  });

  test('1.3 - Login with correct credentials lands on main UI', async ({ page }) => {
    const loginPage = new LoginPage(page);
    const mainLayout = new MainLayout(page);
    
    await loginPage.login('admin', 'admin');
    await mainLayout.waitForMainUI();
    await expect(page).not.toHaveURL(/.*\/login.*/);
  });

  test('1.4 - Session persists after page refresh', async ({ page }) => {
    const loginPage = new LoginPage(page);
    const mainLayout = new MainLayout(page);
    
    await loginPage.login('admin', 'admin');
    await mainLayout.waitForMainUI();
    
    // Refresh page (use load instead of networkidle)
    await page.reload({ waitUntil: 'load' });
    
    // Should still be logged in
    await expect(page).not.toHaveURL(/.*\/login.*/);
    await mainLayout.waitForMainUI();
  });

  test('1.5 - Logout redirects to login page', async ({ page }) => {
    const loginPage = new LoginPage(page);
    const mainLayout = new MainLayout(page);
    
    await loginPage.login('admin', 'admin');
    await mainLayout.waitForMainUI();
    
    // Logout
    await mainLayout.logout();
    
    // Should be redirected to login (check for login form instead of URL)
    await expect(loginPage.submitButton).toBeVisible({ timeout: 5000 });
    await expect(loginPage.usernameInput).toBeVisible();
  });

  test('1.6 - Change password button navigates to change password page', async ({ page }) => {
    const loginPage = new LoginPage(page);
    const mainLayout = new MainLayout(page);
    
    await loginPage.login('admin', 'admin');
    await mainLayout.waitForMainUI();
    
    // Navigate to change password
    await mainLayout.goToChangePassword();
    
    // Should navigate to change password page
    await expect(page).toHaveURL(/.*\/change-password.*/, { timeout: 5000 });
  });

  test('1.8 - Creating user with short password shows validation error', async ({ page }) => {
    const loginPage = new LoginPage(page);
    const mainLayout = new MainLayout(page);
    
    await loginPage.login('admin', 'admin');
    await mainLayout.waitForMainUI();
    
    // Navigate to admin users page
    await mainLayout.goToManageUsers();
    await expect(page).toHaveURL(/.*\/admin\/users.*/, { timeout: 5000 });
    
    // Try to create user with short password
    const usernameInput = page.locator('input[type="text"]').first();
    const passwordInput = page.locator('input[type="password"]').first();
    await usernameInput.fill('testuser');
    await passwordInput.fill('short');
    await page.locator('button:has-text("Create")').click();
    
    // Should show validation error about password length
    await expect(page.locator('.v-alert')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.v-alert')).toContainText(/password|characters|length/i);
  });
});
