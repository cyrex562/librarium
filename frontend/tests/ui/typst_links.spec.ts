import { expect, test, type Page } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';
import { expandPanel } from './helpers/panels';

// Links, outgoing links and backlinks for Typst notes (#144).

const NOTE = 'papers/main.typ';
const CONTENT = [
    '= Main',
    '',
    'See #link("librarium://note/Roadmap")[the roadmap], #link("draft.typ")[the draft],',
    'and #link("https://example.com")[the site].',
].join('\n');

async function setup(page: Page) {
    const now = new Date().toISOString();
    await seedAuthTokens(page);
    await seedActiveVault(page, defaultVault.id);
    await installCommonAppMocks(page, {
        profile: defaultProfile,
        vaults: [defaultVault],
        treeByVaultId: {
            [defaultVault.id]: [
                {
                    name: 'papers',
                    path: 'papers',
                    is_directory: true,
                    modified: now,
                    children: [
                        { name: 'main.typ', path: NOTE, is_directory: false, modified: now },
                        { name: 'draft.typ', path: 'papers/draft.typ', is_directory: false, modified: now },
                    ],
                },
                { name: 'Roadmap.md', path: 'Roadmap.md', is_directory: false, modified: now },
                { name: 'index.md', path: 'index.md', is_directory: false, modified: now },
            ],
        },
        fileContentsByVaultId: {
            [defaultVault.id]: { [NOTE]: CONTENT, 'papers/draft.typ': '= Draft', 'Roadmap.md': '# Roadmap', 'index.md': 'Read [[main.typ]].' },
        },
        backlinksByVaultId: { [defaultVault.id]: [{ path: 'index.md', title: 'index' }] },
    });
    await page.addInitScript(() => {
        (window as unknown as { __opened: string[] }).__opened = [];
        window.open = ((url: string) => {
            (window as unknown as { __opened: string[] }).__opened.push(url);
            return null;
        }) as typeof window.open;
    });
    await page.goto('/');
    await page.locator('.file-tree-node', { hasText: 'papers' }).first().click();
    await page.locator('.file-tree-node', { hasText: 'main.typ' }).click();
    await expect(page.locator('[data-testid="typst-editor"]')).toBeVisible();
}

function tab(page: Page, name: string) {
    return page.locator('.tab-item', { hasText: name });
}

test.describe('Typst note links', () => {
    test('Outgoing Links lists the note\'s links and opens them', async ({ page }) => {
        await setup(page);
        await expandPanel(page, '.outgoing-header');
        const panel = page.locator('.outgoing-links-panel');
        await expect(panel.locator('.link-item')).toHaveText(['the roadmap', 'the draft', 'the site']);

        await panel.locator('.link-item', { hasText: 'the roadmap' }).click();
        await expect(tab(page, 'Roadmap.md')).toBeVisible();

        // Panels start collapsed, and switching note type remounts them.
        await tab(page, 'main.typ').click();
        await expandPanel(page, '.outgoing-header');
        await panel.locator('.link-item', { hasText: 'the draft' }).click();
        // Relative to the linking note: papers/draft.typ, not /draft.typ.
        await expect(tab(page, 'draft.typ')).toBeVisible();

        await tab(page, 'main.typ').click();
        await expandPanel(page, '.outgoing-header');
        await panel.locator('.link-item', { hasText: 'the site' }).click();
        expect(await page.evaluate(() => (window as unknown as { __opened: string[] }).__opened)).toEqual(['https://example.com']);
    });

    test('Backlinks show on a Typst note', async ({ page }) => {
        await setup(page);
        await expandPanel(page, '.backlinks-header');
        await expect(page.locator('.backlinks-panel')).toContainText('index');
    });

    test('links in Preview open notes in Librarium and web pages in a new tab', async ({ page }) => {
        await setup(page);
        await page.route(/.*\/api\/vaults\/[^/]+\/render-typst$/, (route) => route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({
                html: '<p>See <a href="librarium://note/Roadmap">the roadmap</a>, <a href="draft.typ">the draft</a>, '
                    + '<a href="https://example.com">the site</a>, <a href="javascript:alert(1)">bad</a>.</p>',
                css: '',
                diagnostics: [],
            }),
        }));
        await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Preview' }).click();
        const body = page.locator('[data-testid="typst-preview-body"]');
        await expect(body.getByText('the roadmap')).toBeVisible();
        // The sanitizer keeps note links and drops script URLs.
        await expect(body.locator('a', { hasText: 'the roadmap' })).toHaveAttribute('href', 'librarium://note/Roadmap');
        await expect(body.locator('a', { hasText: 'bad' })).not.toHaveAttribute('href', /javascript/);

        await body.getByText('the site').click();
        expect(await page.evaluate(() => (window as unknown as { __opened: string[] }).__opened)).toEqual(['https://example.com']);
        await expect(page).toHaveURL(/127\.0\.0\.1|localhost/);

        await body.getByText('the roadmap').click();
        await expect(tab(page, 'Roadmap.md')).toBeVisible();
    });
});
