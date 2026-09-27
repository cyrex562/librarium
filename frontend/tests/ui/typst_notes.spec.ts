import { expect, test, type Page } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

const NOTE = 'paper.typ';
const CONTENT = [
    '#set page(margin: 2cm)',
    '',
    '= Results',
    '',
    'The *main* finding, see @fig.',
    '$ x^2 $',
].join('\n');

type Mocks = Parameters<typeof installCommonAppMocks>[1];

async function setup(page: Page, extra: Mocks = {}) {
    const files: Record<string, string> = { [NOTE]: CONTENT };
    await seedAuthTokens(page);
    await seedActiveVault(page, defaultVault.id);
    await installCommonAppMocks(page, {
        profile: defaultProfile,
        vaults: [defaultVault],
        treeByVaultId: {
            [defaultVault.id]: [{ name: NOTE, path: NOTE, is_directory: false, modified: new Date().toISOString() }],
        },
        fileContentsByVaultId: { [defaultVault.id]: files },
        ...extra,
    });
    await page.goto('/');
    return files;
}

function editor(page: Page) {
    return page.locator('[data-testid="typst-editor"] .typst-editor');
}

test.describe('Typst notes', () => {
    test('a .typ file opens in the Typst editor with highlighting, text unchanged', async ({ page }) => {
        await setup(page);
        await page.getByText(NOTE).click();

        await expect(editor(page)).toBeVisible();
        // Not the Markdown editor, and not the "binary file" fallback.
        await expect(page.locator('.markdown-editor')).toHaveCount(0);
        await expect(page.getByText('Binary file')).toHaveCount(0);

        await expect(editor(page).locator('.typ-heading')).toHaveText('= Results');
        await expect(editor(page).locator('.typ-keyword')).toHaveText('#set');
        await expect(editor(page).locator('.typ-strong')).toHaveText('*main*');
        await expect(editor(page).locator('.typ-math')).toHaveText('$ x^2 $');
        expect(await editor(page).evaluate((el) => el.textContent)).toBe(CONTENT);
    });

    test('edits autosave the raw text (no frontmatter added)', async ({ page }) => {
        const files = await setup(page);
        await page.getByText(NOTE).click();
        await expect(editor(page).locator('.typ-heading')).toBeVisible();

        await editor(page).locator('.typ-heading').click();
        await page.keyboard.press('End');
        await page.keyboard.type(' and discussion', { delay: 30 });

        await expect(editor(page).locator('.typ-heading')).toHaveText('= Results and discussion');
        await expect.poll(() => files[NOTE], { timeout: 10_000 }).toBe(CONTENT.replace('= Results', '= Results and discussion'));
    });

    test('typing brackets and quotes types exactly what was typed', async ({ page }) => {
        const files = await setup(page);
        await page.getByText(NOTE).click();
        await editor(page).locator('.typ-heading').click();
        await page.keyboard.press('Control+End');
        await page.keyboard.type('\n#link("x")[y] {z}', { delay: 20 });
        await expect.poll(() => files[NOTE], { timeout: 10_000 }).toBe(`${CONTENT}\n#link("x")[y] {z}`);
    });

    test('Plain mode turns highlighting off; Formatted turns it back on', async ({ page }) => {
        await setup(page);
        await page.getByText(NOTE).click();
        await expect(editor(page).locator('.typ-heading')).toBeVisible();

        await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Plain' }).click();
        await expect(editor(page).locator('.typ-heading')).toHaveCount(0);
        expect(await editor(page).evaluate((el) => el.textContent)).toBe(CONTENT);

        await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Formatted' }).click();
        await expect(editor(page).locator('.typ-heading')).toBeVisible();
    });

    test('undo reverts an edit', async ({ page }) => {
        await setup(page);
        await page.getByText(NOTE).click();
        await editor(page).locator('.typ-heading').click();
        await page.keyboard.press('End');
        await page.keyboard.type('!!');
        await expect(editor(page).locator('.typ-heading')).toHaveText('= Results!!');

        await page.locator('[data-testid="typst-editor"] button[title="Undo (Ctrl+Z)"]').dispatchEvent('mousedown');
        await expect(editor(page).locator('.typ-heading')).toHaveText('= Results');
    });

    test('New note keeps a .typ extension and opens the Typst editor', async ({ page }) => {
        const files = await setup(page);
        await page.locator('button[title="New note"]').click();
        const dialog = page.locator('.v-dialog:visible').first();
        await dialog.getByLabel('File name').fill('draft.typ');
        await dialog.getByRole('button', { name: /Create/i }).click();

        await expect(page.locator('.v-tab, .tab-item, [role="tab"]').filter({ hasText: 'draft.typ' }).first()).toBeVisible();
        await expect(editor(page)).toBeVisible();
        expect(Object.keys(files)).toContain('draft.typ');
        expect(Object.keys(files)).not.toContain('draft.typ.md');
    });

    test('New note without an extension is still Markdown', async ({ page }) => {
        const files = await setup(page);
        await page.locator('button[title="New note"]').click();
        const dialog = page.locator('.v-dialog:visible').first();
        await dialog.getByLabel('File name').fill('ideas');
        await dialog.getByRole('button', { name: /Create/i }).click();

        await expect.poll(() => Object.keys(files)).toContain('ideas.md');
    });

    test('Preview shows the server-rendered HTML of the current text', async ({ page }) => {
        await setup(page);
        const sent: string[] = [];
        await page.route(/.*\/api\/vaults\/[^/]+\/render-typst$/, async (route) => {
            const body = route.request().postDataJSON() as { path: string; content: string };
            sent.push(body.content);
            expect(body.path).toBe(NOTE);
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify({
                    html: '<h2>Results</h2><p>The <strong>main</strong> finding.</p><script>window.__xss = 1</script>',
                    css: 'math { color: inherit; }',
                    diagnostics: [],
                }),
            });
        });
        await page.getByText(NOTE).click();
        await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Preview' }).click();

        const body = page.locator('[data-testid="typst-preview-body"]');
        await expect(body.locator('h2')).toHaveText('Results');
        await expect(body.locator('strong')).toHaveText('main');
        await expect(editor(page)).toBeHidden();
        expect(sent[0]).toBe(CONTENT);
        // Server HTML is sanitized before it's inserted.
        expect(await body.locator('script').count()).toBe(0);
        expect(await page.evaluate(() => (window as unknown as { __xss?: number }).__xss)).toBeUndefined();
    });

    test('Preview lists compile errors; clicking one jumps to that line in the editor', async ({ page }) => {
        const files = await setup(page);
        await page.route(/.*\/api\/vaults\/[^/]+\/render-typst$/, (route) => route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({
                html: null,
                css: '',
                diagnostics: [{ severity: 'error', message: 'unknown variable: fig', line: 5, column: 26, hints: ['check the spelling'] }],
            }),
        }));
        await page.getByText(NOTE).click();
        await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Preview' }).click();

        const diag = page.locator('[data-testid="typst-diagnostics"]');
        await expect(diag).toContainText('unknown variable: fig');
        await expect(diag).toContainText('hint: check the spelling');
        await diag.getByRole('button', { name: 'Line 5' }).click();

        // Back in the editor with the caret at line 5, column 26, just before "fig" in "@fig".
        await expect(editor(page)).toBeVisible();
        await page.keyboard.type('X');
        await expect.poll(() => files[NOTE], { timeout: 10_000 }).toContain('see @Xfig.');
    });

    test('Preview explains when the server has no Typst support', async ({ page }) => {
        await setup(page);
        await page.route(/.*\/api\/vaults\/[^/]+\/render-typst$/, (route) => route.fulfill({
            status: 501,
            contentType: 'application/json',
            body: JSON.stringify({ error: 'built without Typst support' }),
        }));
        await page.getByText(NOTE).click();
        await page.locator('[data-testid="typst-editor"]').getByRole('button', { name: 'Preview' }).click();
        await expect(page.locator('[data-testid="typst-preview-unavailable"]')).toContainText('built without Typst support');
    });

    test('Export as PDF sends the current text and downloads the PDF', async ({ page }) => {
        await setup(page);
        let sent: { path: string; content?: string } | null = null;
        await page.route(/.*\/api\/vaults\/[^/]+\/export-pdf$/, async (route) => {
            sent = route.request().postDataJSON();
            await route.fulfill({ status: 200, contentType: 'application/pdf', body: '%PDF-1.7 fake' });
        });
        await page.getByText(NOTE).click();
        await editor(page).locator('.typ-heading').click();
        await page.keyboard.press('End');
        await page.keyboard.type(' now');

        const download = page.waitForEvent('download');
        await page.locator('[data-testid="typst-export-pdf"]').click();
        expect((await download).suggestedFilename()).toBe('paper.pdf');
        expect(sent).toEqual({ path: NOTE, content: CONTENT.replace('= Results', '= Results now') });
    });

    test('Export as PDF with compile errors opens Preview to show them', async ({ page }) => {
        await setup(page);
        await page.route(/.*\/api\/vaults\/[^/]+\/export-pdf$/, (route) => route.fulfill({
            status: 422,
            contentType: 'application/json',
            body: JSON.stringify({ error: 'The note has errors', diagnostics: [{ severity: 'error', message: 'unclosed delimiter', line: 2, column: 1, hints: [] }] }),
        }));
        await page.route(/.*\/api\/vaults\/[^/]+\/render-typst$/, (route) => route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ html: null, css: '', diagnostics: [{ severity: 'error', message: 'unclosed delimiter', line: 2, column: 1, hints: [] }] }),
        }));
        await page.getByText(NOTE).click();
        await page.locator('[data-testid="typst-export-pdf"]').click();
        await expect(page.locator('[data-testid="typst-diagnostics"]')).toContainText('unclosed delimiter');
    });

    test('file tree offers Export as PDF for Typst notes only', async ({ page }) => {
        await setup(page, {
            treeByVaultId: {
                [defaultVault.id]: [
                    { name: NOTE, path: NOTE, is_directory: false, modified: new Date().toISOString() },
                    { name: 'plain.md', path: 'plain.md', is_directory: false, modified: new Date().toISOString() },
                ],
            },
        });
        let exported: string | null = null;
        await page.route(/.*\/api\/vaults\/[^/]+\/export-pdf$/, async (route) => {
            exported = (route.request().postDataJSON() as { path: string; content?: string }).path;
            expect((route.request().postDataJSON() as { content?: string }).content).toBeUndefined();
            await route.fulfill({ status: 200, contentType: 'application/pdf', body: '%PDF-1.7 fake' });
        });

        await page.locator('.file-tree-node', { hasText: 'plain.md' }).click({ button: 'right' });
        await expect(page.locator('[data-testid="ctx-rename"]')).toBeVisible();
        await expect(page.locator('[data-testid="ctx-export-pdf"]')).toHaveCount(0);
        await page.keyboard.press('Escape');

        await page.locator('.file-tree-node', { hasText: NOTE }).click({ button: 'right' });
        const download = page.waitForEvent('download');
        await page.locator('[data-testid="ctx-export-pdf"]').click();
        expect((await download).suggestedFilename()).toBe('paper.pdf');
        expect(exported).toBe(NOTE);
    });
});
