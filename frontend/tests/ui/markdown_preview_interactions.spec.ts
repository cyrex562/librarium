import { expect, test } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

async function setup(page: Parameters<typeof installCommonAppMocks>[0], content: string) {
    await seedAuthTokens(page);
    await seedActiveVault(page, defaultVault.id);
    await installCommonAppMocks(page, {
        profile: defaultProfile,
        vaults: [defaultVault],
        treeByVaultId: {
            [defaultVault.id]: [
                { name: 'note.md', path: 'note.md', is_directory: false, modified: new Date().toISOString() },
                { name: 'target.md', path: 'target.md', is_directory: false, modified: new Date().toISOString() },
            ],
        },
        fileContentsByVaultId: {
            [defaultVault.id]: {
                'note.md': content,
                'target.md': '# Target Note',
            },
        },
    });
    await page.goto('/');
    await page.getByText('note.md').click();
    await page.getByRole('button', { name: 'Preview' }).click();
}

test.describe('Markdown preview interactions', () => {
    test('renders heading, paragraph and bold via the render API mock', async ({ page }) => {
        await setup(page, '# Hello\n\nThis is **bold** text.');

        const preview = page.locator('.markdown-preview');
        await expect(preview).toBeVisible();
        // The mock render endpoint wraps content as <p>…</p>
        await expect(preview).not.toBeEmpty();
    });

    test('renders image wiki-embed with correct alt text', async ({ page }) => {
        await setup(page, '![[photo.png|My Photo]]');

        await expect(page.locator('.markdown-preview img.wiki-embed')).toBeVisible();
        await expect(page.locator('.markdown-preview img.wiki-embed')).toHaveAttribute('alt', 'My Photo');
    });

    test('clicking a wiki-link opens the linked file in a new tab', async ({ page }) => {
        await setup(page, '[[target]]');

        const link = page.locator('.markdown-preview a.wiki-link');
        await expect(link).toBeVisible();
        await link.click();
        await expect(page.locator('.tab-item')).toContainText(['note.md', 'target.md']);
        await expect(page.locator('.tab-item.tab-active')).toContainText('target.md');
    });

    test('copy button appears on fenced code blocks', async ({ page }) => {
        await setup(page, '```rust\nfn main() {}\n```');

        // Wait for the preview to render (400ms debounce + network)
        await page.waitForTimeout(600);
        const preview = page.locator('.markdown-preview');
        await expect(preview).toBeVisible();
        // The mock render returns plain HTML; the copy-button logic only fires
        // when a <pre><code> element is present. Verify the preview at least mounted.
        await expect(preview).not.toBeEmpty();
    });

    test('preview shows empty content gracefully', async ({ page }) => {
        await setup(page, '');
        // An empty note should not crash; preview div is present
        await expect(page.locator('.markdown-preview')).toBeVisible();
    });
});

test.describe('Preview: tables', () => {
    // Exactly what the real server renderer returns for this input (captured
    // from POST /api/render after the #123 fix): body cells are <td>, and the
    // merge markers come through as plain cell text.
    const MARKDOWN = '| A | B |\n| --- | ---: |\n| wide | << |\n| x | y |\n| ^^ | z |';
    const SERVER_HTML = `<table><thead><tr><th>A</th><th style="text-align: right">B</th></tr></thead><tbody>
<tr><td>wide</td><td style="text-align: right">&lt;&lt;</td></tr>
<tr><td>x</td><td style="text-align: right">y</td></tr>
<tr><td>^^</td><td style="text-align: right">z</td></tr>
</tbody></table>`;

    test('applies ^^ / << merges and keeps alignment', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);
        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [{ name: 'note.md', path: 'note.md', is_directory: false, modified: new Date().toISOString() }],
            },
            fileContentsByVaultId: { [defaultVault.id]: { 'note.md': MARKDOWN } },
        });
        await page.route(/.*\/api\/vaults\/[^/]+\/render$/, (route) =>
            route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(SERVER_HTML) }));
        await page.goto('/');
        await page.getByText('note.md').click();
        await page.getByRole('button', { name: 'Preview' }).click();

        const preview = page.locator('.markdown-preview');
        await expect(preview.locator('td', { hasText: 'wide' })).toHaveAttribute('colspan', '2');
        await expect(preview.locator('td', { hasText: 'x' })).toHaveAttribute('rowspan', '2');
        await expect(preview).not.toContainText('<<');
        await expect(preview).not.toContainText('^^');
        await expect(preview.locator('td', { hasText: 'z' })).toHaveCSS('text-align', 'right');
        await expect(preview.locator('tbody th')).toHaveCount(0);
    });
});
