import { expect, test } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';
import { expandPanel } from './helpers/panels';

const tree = [
    { name: 'search-a.md', path: 'search-a.md', is_directory: false, modified: new Date().toISOString() },
    { name: 'search-b.md', path: 'search-b.md', is_directory: false, modified: new Date().toISOString() },
];

const results = [
    {
        path: 'search-a.md',
        title: 'Search A',
        matches: [{ line_number: 2, line_text: 'Some content to find', match_start: 5, match_end: 12 }],
        score: 0.9,
    },
];

async function setup(page: Parameters<typeof installCommonAppMocks>[0]) {
    await seedAuthTokens(page);
    await seedActiveVault(page, defaultVault.id);
    await installCommonAppMocks(page, {
        profile: defaultProfile,
        vaults: [defaultVault],
        treeByVaultId: { [defaultVault.id]: tree },
        fileContentsByVaultId: {
            [defaultVault.id]: {
                'search-a.md': '# Search A\n\nSome content to find.',
                'search-b.md': '# Search B',
            },
        },
        searchResults: results,
        tagsByVaultId: {
            [defaultVault.id]: [
                { tag: 'important', count: 5 },
                { tag: 'archive', count: 2 },
            ],
        },
    });
    await page.goto('/');
}

async function openSearch(page: Parameters<typeof installCommonAppMocks>[0]) {
    await page.locator('button[title="Search (Ctrl+Shift+F)"]').click();
    const input = page.getByRole('textbox', { name: 'Search', exact: true });
    await expect(input).toBeVisible();
    return input;
}

test.describe('Search modal — advanced', () => {
    test('shows result count and match lines', async ({ page }) => {
        await setup(page);
        const input = await openSearch(page);
        const dialog = input.locator('xpath=ancestor::div[contains(@class,"v-overlay")]').first();

        await input.fill('content');
        await input.press('Enter');
        await expect(dialog.getByText('search-a.md')).toBeVisible();
        await expect(dialog.getByText('Some content to find')).toBeVisible();
    });

    test('shows empty state when no results', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);
        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: tree },
            fileContentsByVaultId: { [defaultVault.id]: { 'search-a.md': '# A', 'search-b.md': '# B' } },
            searchResults: [],
        });
        await page.goto('/');

        const input = await openSearch(page);
        await input.fill('zzznoresults');
        await input.press('Enter');
        await expect(page.getByText('No results found.')).toBeVisible({ timeout: 3000 });
    });

    test('tag-based search (#tag syntax) sends query with hash', async ({ page }) => {
        await setup(page);

        const input = await openSearch(page);
        await input.fill('#important');
        await input.press('Enter');

        await expect(input).toHaveValue('#important');
    });

    test('clicking a search result opens the file in a tab', async ({ page }) => {
        await setup(page);
        const input = await openSearch(page);
        await input.fill('content');
        await input.press('Enter');

        const dialog = input.locator('xpath=ancestor::div[contains(@class,"v-overlay")]').first();
        await dialog.getByText('search-a.md').click();
        await expect(page.locator('.tab-item.tab-active')).toContainText('search-a.md');
    });

    test('tag panel click populates search with tag query', async ({ page }) => {
        await setup(page);

        // click a tag in the sidebar tags panel
        await expandPanel(page, '.tags-header');
        await page.locator('.tag-item', { hasText: 'important' }).click();

        const input = page.getByRole('textbox', { name: 'Search', exact: true });
        await expect(input).toBeVisible();
        await expect(input).toHaveValue('#important');
    });
});
