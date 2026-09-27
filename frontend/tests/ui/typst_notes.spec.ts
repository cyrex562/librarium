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
});
