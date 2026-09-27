import { expect, test, type Page } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

// #155: CodeJar's addClosing inserted a closing bracket/quote that typing
// never overtyped, so `[a](b)` saved as `[a](b))]` and apostrophes doubled.

const NOTE = 'typing.md';
const START = 'start';
const TYPED = ` [link](https://x.y) "quoted" don't {braces} it's (a [nested] one)`;

async function setup(page: Page) {
    const files: Record<string, string> = { [NOTE]: START };
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
    await page.goto('/');
    await page.getByText(NOTE).click();
    await expect(page.locator('.markdown-editor')).toContainText(START);
    return files;
}

async function typeAtEnd(page: Page, text: string) {
    await page.locator('.markdown-editor').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type(text, { delay: 20 });
}

test.describe('Markdown editor typing', () => {
    test('Formatted mode saves brackets and quotes exactly as typed', async ({ page }) => {
        const files = await setup(page);
        await typeAtEnd(page, TYPED);
        await expect.poll(() => files[NOTE], { timeout: 10_000 }).toBe(START + TYPED);
    });

    test('Plain mode saves brackets and quotes exactly as typed', async ({ page }) => {
        const files = await setup(page);
        await page.getByRole('button', { name: 'Plain' }).click();
        await typeAtEnd(page, TYPED);
        await expect.poll(() => files[NOTE], { timeout: 10_000 }).toBe(START + TYPED);
    });
});
