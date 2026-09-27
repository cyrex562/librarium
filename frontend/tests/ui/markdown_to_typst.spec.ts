import { expect, test, type Page } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

// Convert to Typst (#141) and Export as PDF for Markdown notes (#140).

const NOTE = 'ideas.md';
const CONTENT = '# Ideas\n\nSome **bold** thinking.';

async function setup(page: Page) {
    const files: Record<string, string> = { [NOTE]: CONTENT };
    const requests: string[] = [];
    await seedAuthTokens(page);
    await seedActiveVault(page, defaultVault.id);
    await installCommonAppMocks(page, {
        profile: defaultProfile,
        vaults: [defaultVault],
        treeByVaultId: {
            [defaultVault.id]: [{ name: NOTE, path: NOTE, is_directory: false, modified: new Date().toISOString() }],
        },
        fileContentsByVaultId: { [defaultVault.id]: files },
    });
    page.on('request', (r) => {
        if (r.url().includes('/api/vaults/')) requests.push(`${r.method()} ${new URL(r.url()).pathname}`);
    });
    // The conversion itself is the server's (tested in Rust); here the mock
    // plays the server: it writes the .typ into the mocked vault.
    await page.route(/.*\/api\/vaults\/[^/]+\/convert-to-typst$/, async (route) => {
        const { path } = route.request().postDataJSON() as { path: string };
        const target = path.replace(/\.md$/, '.typ');
        files[target] = '= Ideas\n\nSome #strong[bold] thinking.\n';
        await route.fulfill({
            status: 201,
            contentType: 'application/json',
            body: JSON.stringify({
                path: target,
                warnings: [
                    { line: 3, message: 'HTML has no Typst equivalent; kept as literal text: <div>' },
                    { line: null, message: 'Remote image https://x.y/a.png can\'t be embedded' },
                ],
            }),
        });
    });
    await page.goto('/');
    return { files, requests };
}

test.describe('Markdown to Typst', () => {
    test('file tree: Convert to Typst opens the new note and reports what was lost', async ({ page }) => {
        await setup(page);
        await page.locator('.file-tree-node', { hasText: NOTE }).click({ button: 'right' });
        await page.locator('[data-testid="ctx-convert-typst"]').click();

        const report = page.locator('[data-testid="conversion-report"]');
        await expect(report).toContainText('Created ideas.typ from ideas.md');
        await expect(report).toContainText('Line 3: HTML has no Typst equivalent');
        await expect(report).toContainText('Remote image');
        await report.getByRole('button', { name: 'OK' }).click();
        await expect(report).toHaveCount(0);

        await expect(page.locator('.tab-item', { hasText: 'ideas.typ' })).toBeVisible();
        await expect(page.locator('[data-testid="typst-editor"] .typst-editor')).toContainText('#strong[bold]');
    });

    test('file tree: Convert to Typst is only offered for Markdown notes', async ({ page }) => {
        await setup(page);
        await page.locator('.file-tree-node', { hasText: NOTE }).click({ button: 'right' });
        await expect(page.locator('[data-testid="ctx-convert-typst"]')).toBeVisible();
        await expect(page.locator('[data-testid="ctx-export-pdf"]')).toBeVisible();
    });

    test('editor: Convert to Typst saves pending edits before converting', async ({ page }) => {
        const { files, requests } = await setup(page);
        await page.getByText(NOTE).click();
        await page.locator('.markdown-editor').click();
        await page.keyboard.press('Control+End');
        await page.keyboard.type(' More.');
        // Straight away, well inside the 2 s auto-save delay.
        await page.getByTitle('More options').click();
        await page.locator('[data-testid="toolbar-convert-typst"]').click();

        await expect(page.locator('[data-testid="conversion-report"]')).toBeVisible();
        expect(files[NOTE]).toBe(`${CONTENT} More.`);
        const save = requests.findIndex((r) => r === `PUT /api/vaults/${defaultVault.id}/files/${NOTE}`);
        const convert = requests.findIndex((r) => r.endsWith('/convert-to-typst'));
        expect(save).toBeGreaterThanOrEqual(0);
        expect(convert).toBeGreaterThan(save);
    });

    test('editor: Export as PDF sends the Markdown text and downloads a PDF', async ({ page }) => {
        await setup(page);
        let sent: { path: string; content?: string } | null = null;
        await page.route(/.*\/api\/vaults\/[^/]+\/export-pdf$/, async (route) => {
            sent = route.request().postDataJSON();
            await route.fulfill({ status: 200, contentType: 'application/pdf', body: '%PDF-1.7 fake' });
        });
        await page.getByText(NOTE).click();
        await expect(page.locator('.markdown-editor')).toContainText('Ideas');

        await page.getByTitle('More options').click();
        const download = page.waitForEvent('download');
        await page.locator('[data-testid="toolbar-export-pdf"]').click();
        expect((await download).suggestedFilename()).toBe('ideas.pdf');
        expect(sent).toEqual({ path: NOTE, content: CONTENT });
    });
});
