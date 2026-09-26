import { expect, test } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';
import { expandPanel } from './helpers/panels';

const FILE_A = 'notes/alpha.md';
const FILE_B = 'notes/beta.md';
const FILE_C = 'notes/gamma.md';

const tree = [
    { name: 'notes', path: 'notes', is_directory: true, modified: new Date().toISOString() },
    { name: 'alpha.md', path: FILE_A, is_directory: false, modified: new Date().toISOString() },
    { name: 'beta.md', path: FILE_B, is_directory: false, modified: new Date().toISOString() },
    { name: 'gamma.md', path: FILE_C, is_directory: false, modified: new Date().toISOString() },
];

const fileContents = {
    [FILE_A]: '# Alpha\n\nLinks to [[beta]] and [[gamma]].\n\n#research #draft',
    [FILE_B]: '# Beta\n',
    [FILE_C]: '# Gamma\n',
};

async function setupWithPanels(page: Parameters<typeof installCommonAppMocks>[0]) {
    await seedAuthTokens(page);
    await seedActiveVault(page, defaultVault.id);
    await installCommonAppMocks(page, {
        profile: defaultProfile,
        vaults: [defaultVault],
        treeByVaultId: { [defaultVault.id]: tree },
        fileContentsByVaultId: { [defaultVault.id]: fileContents },
        bookmarksByVaultId: {
            [defaultVault.id]: [
                { id: 'bm-1', path: FILE_B, title: 'beta' },
            ],
        },
        tagsByVaultId: {
            [defaultVault.id]: [
                { tag: 'research', count: 3 },
                { tag: 'draft', count: 1 },
            ],
        },
        backlinksByVaultId: {
            [defaultVault.id]: [
                { path: FILE_B, title: 'Beta' },
            ],
        },
    });
    await page.goto('/');
}

test.describe('Bookmarks panel', () => {
    test('shows existing bookmarks and opens file on click', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText(FILE_A.split('/').pop()!).click(); // open alpha
        await expandPanel(page, '.bookmarks-header');

        await expect(page.getByText('BOOKMARKS')).toBeVisible();
        await expect(page.locator('.bookmark-item')).toContainText('beta');

        await page.locator('.bookmark-item').first().click();
        await expect(page.locator('.tab-item.tab-active')).toContainText('beta');
    });

    test('adds a bookmark for the current file', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText('alpha.md').click();
        await expandPanel(page, '.bookmarks-header');

        // click the bookmark-plus button (stop propagation avoids header toggle)
        await page.locator('button[title="Bookmark current file"]').click();
        // new bookmark appears in the list
        await expect(page.locator('.bookmark-item')).toHaveCount(2);
    });

    test('removes a bookmark via the × button', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText('alpha.md').click();
        await expandPanel(page, '.bookmarks-header');

        await expect(page.locator('.bookmark-item')).toHaveCount(1);
        await page.locator('.bookmark-item button[title="Remove bookmark"]').first().click();
        await expect(page.locator('.bookmark-item')).toHaveCount(0);
    });

    test('collapses and expands by clicking the BOOKMARKS header', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText('alpha.md').click();
        await expandPanel(page, '.bookmarks-header');

        await expect(page.locator('.bookmark-item')).toBeVisible();
        // click header to collapse
        await page.locator('.bookmarks-header').click();
        await expect(page.locator('.bookmark-item')).not.toBeVisible();
        // click again to expand
        await page.locator('.bookmarks-header').click();
        await expect(page.locator('.bookmark-item')).toBeVisible();
    });
});

test.describe('Tags panel', () => {
    test('shows tags sorted by count', async ({ page }) => {
        await setupWithPanels(page);
        await expandPanel(page, '.tags-header');

        await expect(page.getByText('TAGS')).toBeVisible();
        const items = page.locator('.tag-item');
        await expect(items).toHaveCount(2);
        // 'research' has count 3, should be first
        await expect(items.first()).toContainText('research');
    });

    test('clicking a tag opens search with #tag query', async ({ page }) => {
        await setupWithPanels(page);
        await expandPanel(page, '.tags-header');
        // Click a tag — the sidebar emits a 'search' event which should open the search modal
        await page.locator('.tag-item', { hasText: 'research' }).click();
        const input = page.getByRole('textbox', { name: 'Search', exact: true });
        await expect(input).toBeVisible();
        await expect(input).toHaveValue('#research');
    });

    test('collapses on header click', async ({ page }) => {
        await setupWithPanels(page);
        await expandPanel(page, '.tags-header');
        await expect(page.locator('.tag-item').first()).toBeVisible();
        await page.locator('.tags-header').click();
        await expect(page.locator('.tag-item').first()).not.toBeVisible();
    });
});

test.describe('Backlinks panel', () => {
    test('shows backlinks for the open file', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText('alpha.md').click();
        await expandPanel(page, '.backlinks-header');

        await expect(page.getByText('BACKLINKS')).toBeVisible();
        await expect(page.locator('.backlink-item')).toHaveCount(1);
        await expect(page.locator('.backlink-item')).toContainText('Beta');
    });

    test('opens a backlinked file on click', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText('alpha.md').click();
        await expandPanel(page, '.backlinks-header');

        await page.locator('.backlink-item').first().click();
        await expect(page.locator('.tab-item.tab-active')).toContainText('beta');
    });

    test('collapses backlinks panel on header click', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText('alpha.md').click();
        await expandPanel(page, '.backlinks-header');

        await expect(page.locator('.backlink-item').first()).toBeVisible();
        await page.locator('.backlinks-header').click();
        await expect(page.locator('.backlink-item').first()).not.toBeVisible();
    });

    test('shows empty state when no backlinks', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);
        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [{ name: 'solo.md', path: 'solo.md', is_directory: false, modified: new Date().toISOString() }] },
            fileContentsByVaultId: { [defaultVault.id]: { 'solo.md': '# Solo' } },
            backlinksByVaultId: { [defaultVault.id]: [] },
        });
        await page.goto('/');
        await page.getByText('solo.md').click();
        await expandPanel(page, '.backlinks-header');

        await expect(page.getByText('No notes link to this file yet.')).toBeVisible();
    });
});

test.describe('Outline panel', () => {
    test('shows headings extracted from file content', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);
        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [{ name: 'outline-note.md', path: 'outline-note.md', is_directory: false, modified: new Date().toISOString() }] },
            fileContentsByVaultId: {
                [defaultVault.id]: {
                    'outline-note.md': '# Introduction\n\n## Background\n\n### Detail\n\nBody text.',
                },
            },
        });
        await page.goto('/');
        await page.getByText('outline-note.md').click();

        const outlinePanel = page.locator('.outline-panel');
        await expect(outlinePanel.getByText('OUTLINE', { exact: true })).toBeVisible();
        await expect(outlinePanel.locator('.outline-item', { hasText: 'Introduction' })).toBeVisible();
        await expect(outlinePanel.locator('.outline-item', { hasText: 'Background' })).toBeVisible();
        await expect(outlinePanel.locator('.outline-item', { hasText: 'Detail' })).toBeVisible();
    });

    test('shows empty state when no headings', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);
        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [{ name: 'plain.md', path: 'plain.md', is_directory: false, modified: new Date().toISOString() }] },
            fileContentsByVaultId: { [defaultVault.id]: { 'plain.md': 'Just a paragraph, no headings.' } },
        });
        await page.goto('/');
        await page.getByText('plain.md').click();

        await expect(page.locator('.outline-panel').getByText('No headings')).toBeVisible();
    });
});

test.describe('Outgoing links panel', () => {
    test('shows internal and external links from content', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);
        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [{ name: 'links.md', path: 'links.md', is_directory: false, modified: new Date().toISOString() }] },
            fileContentsByVaultId: {
                [defaultVault.id]: {
                    'links.md': '# Links\n\n[[internal-note]]\n\n[External](https://example.com)',
                },
            },
        });
        await page.goto('/');
        await page.getByText('links.md').click();
        await expandPanel(page, '.outgoing-header');

        const outgoingPanel = page.locator('.outgoing-links-panel');
        await expect(outgoingPanel.getByText('OUTGOING LINKS', { exact: true })).toBeVisible();
        await expect(outgoingPanel.locator('.link-item', { hasText: 'internal-note' })).toBeVisible();
        await expect(outgoingPanel.locator('.link-item', { hasText: 'External' })).toBeVisible();
    });

    test('opens internal wiki-link file on click', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText('alpha.md').click();
        await expandPanel(page, '.outgoing-header');

        // alpha.md has [[beta]] and [[gamma]]
        const betaLink = page.locator('.link-item', { hasText: 'beta' }).first();
        await expect(betaLink).toBeVisible();
        await betaLink.click();
        await expect(page.locator('.tab-item.tab-active')).toContainText('beta.md');
    });
});

test.describe('Neighboring files panel', () => {
    test('shows previous and next markdown files', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText('beta.md').click();
        await expandPanel(page, '.neighboring-header');

        await expect(page.getByText('NEIGHBORING FILES')).toBeVisible();
        // beta is between alpha and gamma in the flat tree
        await expect(page.locator('.neighbor-item', { hasText: 'alpha' })).toBeVisible();
        await expect(page.locator('.neighbor-item', { hasText: 'gamma' })).toBeVisible();
    });

    test('navigates to neighboring file on click', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText('beta.md').click();
        await expandPanel(page, '.neighboring-header');

        await page.locator('.neighbor-item', { hasText: 'alpha' }).click();
        await expect(page.locator('.tab-item.tab-active')).toContainText('alpha');
    });
});

test.describe('Recent files panel', () => {
    test('shows recently opened files', async ({ page }) => {
        await setupWithPanels(page);

        // Open two files to populate recent list
        await page.getByText('alpha.md').click();
        await page.getByText('beta.md').click();
        await expandPanel(page, '.recent-header');

        await expect(page.locator('.recent-files-panel').getByText('RECENT FILES', { exact: true })).toBeVisible();
        // Recent files should include the opened files
        await expect(page.locator('.recent-item')).toHaveCount(2);
    });

    test('opens file from recent list', async ({ page }) => {
        await setupWithPanels(page);
        await page.getByText('alpha.md').click();
        await page.getByText('beta.md').click();
        await expandPanel(page, '.recent-header');

        // click a recent item
        const recentItem = page.locator('.recent-item').first();
        await expect(recentItem).toBeVisible();
        await recentItem.click();
        await expect(page.locator('.tab-item.tab-active')).toContainText('beta.md');
    });
});
