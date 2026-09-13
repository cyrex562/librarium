import { expect, test } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

const NOTE = 'insight-note.md';
const CONTENT = '# Intro\n\nSome text.\n\n## Details\n\nMore text.';

async function setup(page: Parameters<typeof installCommonAppMocks>[0]) {
    await seedAuthTokens(page);
    await seedActiveVault(page, defaultVault.id);
    await installCommonAppMocks(page, {
        profile: defaultProfile,
        vaults: [defaultVault],
        treeByVaultId: {
            [defaultVault.id]: [{ name: NOTE, path: NOTE, is_directory: false, modified: new Date().toISOString() }],
        },
        fileContentsByVaultId: { [defaultVault.id]: { [NOTE]: CONTENT } },
        outlineResult: {
            summary: 'This note introduces a concept and provides details.',
            sections: [
                { line_number: 1, level: 1, title: 'Intro', summary: 'Introductory section' },
                { line_number: 5, level: 2, title: 'Details', summary: 'Detailed section' },
            ],
        },
    });
    await page.goto('/');
    await page.getByText(NOTE).click();
}

async function openAiInsights(page: Parameters<typeof installCommonAppMocks>[0]) {
    const header = page.locator('.ml-insights-panel .ml-header');
    await expect(header).toBeVisible();
    // The panel defaults to expanded (MlInsightsPanel.vue: `const expanded =
    // ref(true)`), and the header is a toggle — so clicking unconditionally
    // COLLAPSES it and hides the buttons these tests then assert on. Only click
    // when it is actually closed, which keeps this helper correct whichever way
    // the default goes later.
    const chevron = page.locator('.ml-insights-panel .ml-header .mdi-chevron-right');
    if (await chevron.isVisible().catch(() => false)) {
        await header.click();
    }
    await expect(page.locator('.ml-insights-panel .mdi-chevron-down')).toBeVisible();
}

test.describe('AI Insights panel', () => {
    test('shows the AI INSIGHTS panel with Generate outline button', async ({ page }) => {
        await setup(page);
        await expect(page.getByText('AI INSIGHTS')).toBeVisible();
        await openAiInsights(page);
        await expect(page.getByRole('button', { name: 'Generate outline' })).toBeVisible();
        await expect(page.getByRole('button', { name: 'Suggest organization' })).toBeVisible();
    });

    test('clicking Generate outline shows summary and sections', async ({ page }) => {
        await setup(page);
        await openAiInsights(page);
        await page.getByRole('button', { name: 'Generate outline' }).click();

        const panel = page.locator('.ml-insights-panel');
        await expect(panel.getByText('This note introduces a concept and provides details.')).toBeVisible();
        await expect(panel.locator('.outline-item')).toContainText(['Intro', 'Details']);
    });

    test('clicking Suggest organization shows suggestions', async ({ page }) => {
        await setup(page);
        await openAiInsights(page);
        await page.getByRole('button', { name: 'Suggest organization' }).click();

        const panel = page.locator('.ml-insights-panel');
        await expect(panel.getByText('Move to subfolder')).toBeVisible();
        await expect(panel.getByText('Add tags')).toBeVisible();
    });
});
