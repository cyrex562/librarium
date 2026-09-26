import { expect, test, type Page } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

const NOTE = 'table-note.md';
const CONTENT = [
    'Intro paragraph.',
    '',
    '| Fruit | Qty | Price |',
    '| :---- | :-: | ----: |',
    '| Apple | 3 | 1.50 |',
    '| Pear | 10 | 0.75 |',
    '',
    'Outro paragraph.',
].join('\n');

async function openNote(page: Page) {
    await seedAuthTokens(page);
    await seedActiveVault(page, defaultVault.id);
    await installCommonAppMocks(page, {
        profile: defaultProfile,
        vaults: [defaultVault],
        treeByVaultId: {
            [defaultVault.id]: [{ name: NOTE, path: NOTE, is_directory: false, modified: new Date().toISOString() }],
        },
        fileContentsByVaultId: { [defaultVault.id]: { [NOTE]: CONTENT } },
    });
    await page.goto('/');
    await page.getByText(NOTE).click();
    await expect(page.locator('.markdown-editor .editor-md-table')).toBeVisible();
}

/** The editor's textContent IS the document (CodeJar reads it back). */
function editorText(page: Page) {
    return page.locator('.markdown-editor').evaluate((el) => el.textContent ?? '');
}

function cell(page: Page, text: string) {
    return page.locator('.markdown-editor .editor-md-table-cell', { hasText: text }).first();
}

test.describe('Formatted mode: tables render as a grid', () => {
    test('renders a grid with the markdown syntax hidden, source unchanged', async ({ page }) => {
        await openNote(page);

        await expect(page.locator('.markdown-editor .editor-md-table-row')).toHaveCount(4);
        await expect(page.locator('.markdown-editor .editor-md-table-row.is-divider')).toBeHidden();
        await expect(page.locator('.markdown-editor .editor-md-table-pipe').first()).toBeHidden();
        await expect(cell(page, 'Apple')).toBeVisible();
        await expect(page.locator('.markdown-editor .editor-md-table-row.is-header .editor-md-table-cell').first())
            .toHaveCSS('font-weight', '600');

        expect(await editorText(page)).toBe(CONTENT);
    });

    test('typing into a cell edits that cell in the markdown', async ({ page }) => {
        await openNote(page);

        await cell(page, 'Apple').click();
        await page.keyboard.press('End');
        // The delay lets the editor re-render and restore the caret between
        // keystrokes — that restore used to drop the caret into the hidden
        // pipe after the cell (see editor/table-caret.ts).
        await page.keyboard.type('pie', { delay: 120 });

        await expect.poll(() => editorText(page)).toContain('| Apple pie| 3 | 1.50 |');
        await expect(cell(page, 'Apple pie')).toBeVisible();
        await expect(page.locator('.markdown-editor .editor-md-table-row')).toHaveCount(4);
    });

    test('Tab moves to the next cell', async ({ page }) => {
        await openNote(page);

        await cell(page, 'Apple').click();
        await page.keyboard.press('Tab');
        await page.keyboard.type('7');

        // Tab from the Fruit cell lands in the Qty cell of the same row.
        await expect.poll(async () => (await editorText(page)).split('\n')[4]).toMatch(/^\|\s*Apple\s*\|[^|]*7[^|]*\|/);
    });

    test('the table toolbar still works: add row below', async ({ page }) => {
        await openNote(page);

        await cell(page, 'Pear').click();
        await page.getByTitle('Add row below').click();

        await expect(page.locator('.markdown-editor .editor-md-table-row')).toHaveCount(5);
        await expect.poll(async () => (await editorText(page)).split('\n').filter((l) => l.startsWith('|')).length).toBe(5);
    });

    test('Plain view shows the raw markdown, not a grid', async ({ page }) => {
        await openNote(page);

        await page.getByRole('button', { name: 'Plain' }).click();

        await expect(page.locator('.markdown-editor .editor-md-table')).toHaveCount(0);
        await expect(page.locator('.markdown-editor')).toContainText('| Apple | 3 | 1.50 |');
    });
});
