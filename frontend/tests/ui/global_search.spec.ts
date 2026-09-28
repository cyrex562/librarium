import { expect, test } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

test.describe('Global search modal', () => {
    test('searches and opens selected result', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [{ name: 'todo.md', path: 'todo.md', is_directory: false, modified: new Date().toISOString() }],
            },
            fileContentsByVaultId: {
                [defaultVault.id]: {
                    'todo.md': '# Todo\n- [ ] ship tests',
                },
            },
            searchResults: [
                {
                    path: 'todo.md',
                    title: 'todo',
                    matches: [{ line_number: 1, line_text: '# Todo', match_start: 2, match_end: 6 }],
                    score: 1,
                },
            ],
        });

        await page.goto('/');
        await page.locator('button[title="Search (Ctrl+Shift+F)"]').click();

        await page.getByRole('textbox', { name: 'Search', exact: true }).fill('todo');
        await page.keyboard.press('Enter');

        const modal = page.locator('.v-dialog:visible').first();
        await expect(modal.getByText('todo.md')).toBeVisible();
        await modal.getByText('todo.md').first().click();
        await expect(page.locator('.tab-item')).toContainText('todo.md');
    });

    test('a Typst note in the results opens in the Typst editor (#143)', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);
        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [{ name: 'paper.typ', path: 'paper.typ', is_directory: false, modified: new Date().toISOString() }],
            },
            fileContentsByVaultId: { [defaultVault.id]: { 'paper.typ': '= Findings\n\nA *walrus* result.' } },
            searchResults: [
                {
                    path: 'paper.typ',
                    title: 'paper',
                    matches: [{ line_number: 3, line_text: 'A walrus result.', match_start: 2, match_end: 8 }],
                    score: 1,
                },
            ],
        });

        await page.goto('/');
        await page.locator('button[title="Search (Ctrl+Shift+F)"]').click();
        await page.getByRole('textbox', { name: 'Search', exact: true }).fill('walrus');
        await page.keyboard.press('Enter');

        const modal = page.locator('.v-dialog:visible').first();
        await expect(modal.getByText('A walrus result.')).toBeVisible();
        await modal.getByText('paper.typ').first().click();
        await expect(page.locator('.tab-item')).toContainText('paper.typ');
        await expect(page.locator('[data-testid="typst-editor"] .typ-strong')).toHaveText('*walrus*');
    });
});
