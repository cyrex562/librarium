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

test.describe('Formatted mode: merged cells', () => {
    const MERGED = [
        '| Region | Q1 | Q2 |',
        '| --- | --- | --- |',
        '| North | 10 | 12 |',
        '| ^^ | 11 | 13 |',
        '| Total | 44 | << |',
    ].join('\n');

    async function openMerged(page: Page, content: string) {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);
        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [{ name: NOTE, path: NOTE, is_directory: false, modified: new Date().toISOString() }],
            },
            fileContentsByVaultId: { [defaultVault.id]: { [NOTE]: content } },
        });
        await page.goto('/');
        await page.getByText(NOTE).click();
        await expect(page.locator('.markdown-editor .editor-md-table')).toBeVisible();
    }

    test('^^ and << span the anchor cell and hide the marker cells', async ({ page }) => {
        await openMerged(page, MERGED);

        const north = await cell(page, 'North').boundingBox();
        const ten = await cell(page, '10').boundingBox();
        const eleven = await cell(page, '11').boundingBox();
        // North spans the 10 and 11 rows.
        expect(north!.y).toBeCloseTo(ten!.y, 0);
        expect(north!.height).toBeCloseTo(eleven!.y + eleven!.height - ten!.y, 0);

        const total44 = await cell(page, '44').boundingBox();
        const q1 = await cell(page, 'Q1').boundingBox();
        const q2 = await cell(page, 'Q2').boundingBox();
        // 44 spans the Q1 and Q2 columns.
        expect(total44!.x).toBeCloseTo(q1!.x, 0);
        expect(total44!.width).toBeCloseTo(q2!.x + q2!.width - q1!.x, 0);

        await expect(page.locator('.markdown-editor .editor-md-table-cell.is-merged')).toHaveCount(2);
        await expect(page.locator('.markdown-editor .editor-md-table-cell.is-merged').first()).toBeHidden();
        expect(await editorText(page)).toBe(MERGED);
    });

    test('typing ^^ into a cell merges it into the cell above', async ({ page }) => {
        await openMerged(page, '| a | b |\n| --- | --- |\n| top | 1 |\n| low | 2 |');

        await cell(page, 'low').click();
        await page.keyboard.press('End');
        for (let i = 0; i < 4; i += 1) await page.keyboard.press('Backspace', { delay: 80 }); // "low " -> ""
        await page.keyboard.type('^^', { delay: 120 });

        await expect.poll(() => editorText(page)).toContain('| ^^| 2 |');
        await expect(page.locator('.markdown-editor .editor-md-table-cell.is-merged')).toHaveCount(1);
        await expect(cell(page, 'top')).toHaveAttribute('style', /grid-row: span 2/);
    });
});

test.describe('Editor toolbar: no layout shift entering a table', () => {
    test('the toolbar keeps its height and the table does not move', async ({ page }) => {
        await page.setViewportSize({ width: 1280, height: 800 });
        await openNote(page);
        const toolbar = page.locator('.editor-toolbar');
        const context = page.locator('.toolbar-row-context');

        await expect(context).toHaveAttribute('data-context', 'default');
        await expect(page.getByTitle('Code block')).toBeVisible();
        const toolbarBefore = await toolbar.boundingBox();
        const cellBefore = await cell(page, 'Apple').boundingBox();

        await cell(page, 'Apple').click();

        await expect(context).toHaveAttribute('data-context', 'table');
        await expect(page.getByTitle('Add row below')).toBeVisible();
        expect((await toolbar.boundingBox())!.height).toBe(toolbarBefore!.height);
        expect((await cell(page, 'Apple').boundingBox())!.y).toBe(cellBefore!.y);
    });

    test('double-clicking a word in a table selects it in the clicked cell', async ({ page }) => {
        // Before the fixed-height toolbar, the first click grew the toolbar
        // and the second click landed on the row above.
        await page.setViewportSize({ width: 1280, height: 800 });
        await openNote(page);

        await cell(page, 'Pear').dblclick();
        await page.keyboard.type('Plum');

        await expect.poll(() => editorText(page)).toContain('| Plum | 10 | 0.75 |');
    });
});
