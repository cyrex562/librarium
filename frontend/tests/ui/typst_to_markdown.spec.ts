import { expect, test, type Page } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

// Typst → Markdown (#141) and convert-on-import (#142). The conversion itself
// is the server's (tested in Rust); the mock plays the server.

const NOTE = 'essay.typ';

async function setup(page: Page, tree = [{ name: NOTE, path: NOTE, is_directory: false, modified: new Date().toISOString() }]) {
    const files: Record<string, string> = { [NOTE]: '= Essay\n\nText.' };
    const calls: Array<{ path: string; replace: boolean }> = [];
    await seedAuthTokens(page);
    await seedActiveVault(page, defaultVault.id);
    await installCommonAppMocks(page, {
        profile: defaultProfile,
        vaults: [defaultVault],
        treeByVaultId: { [defaultVault.id]: tree },
        fileContentsByVaultId: { [defaultVault.id]: files },
    });
    await page.route(/.*\/api\/vaults\/[^/]+\/convert-to-markdown$/, async (route) => {
        const body = route.request().postDataJSON() as { path: string; replace: boolean };
        calls.push(body);
        const target = body.path.replace(/\.typ$/, '.md');
        files[target] = '# Essay\n\nText.\n';
        await route.fulfill({
            status: 201,
            contentType: 'application/json',
            body: JSON.stringify({
                path: target,
                warnings: [{ line: 1, message: 'A `#set` rule has no Markdown equivalent; kept as a comment: #set page(..)' }],
            }),
        });
    });
    await page.goto('/');
    return { files, calls };
}

test.describe('Typst to Markdown', () => {
    test('file tree: Convert to Markdown opens the Markdown note and reports losses', async ({ page }) => {
        const { calls } = await setup(page);
        await page.locator('.file-tree-node', { hasText: NOTE }).click({ button: 'right' });
        await page.locator('[data-testid="ctx-convert-markdown"]').click();

        const report = page.locator('[data-testid="conversion-report"]');
        await expect(report).toContainText('Converted to Markdown');
        await expect(report).toContainText('Line 1: A `#set` rule');
        await expect(report).toContainText('The original is unchanged.');
        await report.getByRole('button', { name: 'OK' }).click();
        await expect(page.locator('.tab-item', { hasText: 'essay.md' })).toBeVisible();
        expect(calls).toEqual([{ path: NOTE, replace: false }]);
    });

    test('Typst editor: Convert to Markdown saves pending edits first', async ({ page }) => {
        const { files, calls } = await setup(page);
        const order: string[] = [];
        page.on('request', (r) => {
            if (r.method() === 'PUT' && r.url().includes(`/files/${NOTE}`)) order.push('save');
            if (r.url().endsWith('/convert-to-markdown')) order.push('convert');
        });
        await page.getByText(NOTE).click();
        const editor = page.locator('[data-testid="typst-editor"] .typst-editor');
        await editor.locator('.typ-heading').click();
        await page.keyboard.press('End');
        await page.keyboard.type(' two');
        await page.locator('[data-testid="typst-convert-markdown"]').click();

        await expect(page.locator('[data-testid="conversion-report"]')).toBeVisible();
        expect(files[NOTE]).toBe('= Essay two\n\nText.');
        expect(order).toEqual(['save', 'convert']);
        expect(calls).toHaveLength(1);
    });

    test('import: "Convert Typst files to Markdown" replaces imported .typ notes', async ({ page }) => {
        const { calls } = await setup(page, []);
        await page.locator('button[title="Import files or folders"]').click();
        await page.setInputFiles('[data-testid="import-files-input"]', [
            { name: 'paper.typ', mimeType: 'text/plain', buffer: Buffer.from('= Paper') },
            { name: 'notes.md', mimeType: 'text/markdown', buffer: Buffer.from('# Notes') },
        ]);
        await page.locator('[data-testid="import-convert-typst"] input').check();
        await page.getByRole('button', { name: 'Import 2' }).click();

        await expect(page.getByText('Converted 1 Typst file to Markdown.')).toBeVisible();
        expect(calls).toEqual([{ path: 'paper.typ', replace: true }]);
        const report = page.locator('[data-testid="conversion-report"]');
        await expect(report).toContainText('paper.typ: A `#set` rule');
        await expect(report).toContainText('The originals were replaced.');
    });

    test('import: the option only appears when Typst files or archives are queued', async ({ page }) => {
        await setup(page, []);
        await page.locator('button[title="Import files or folders"]').click();
        await page.setInputFiles('[data-testid="import-files-input"]', [
            { name: 'notes.md', mimeType: 'text/markdown', buffer: Buffer.from('# Notes') },
        ]);
        await expect(page.getByText('1 queued')).toBeVisible();
        await expect(page.locator('[data-testid="import-convert-typst"]')).toHaveCount(0);
    });
});
