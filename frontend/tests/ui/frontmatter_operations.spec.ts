import { expect, test } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';
import { expandFrontmatter } from './helpers/panels';

const NOTE = 'fm-test.md';

async function setup(page: Parameters<typeof installCommonAppMocks>[0], frontmatter: Record<string, unknown> = {}) {
    await seedAuthTokens(page);
    await seedActiveVault(page, defaultVault.id);
    await installCommonAppMocks(page, {
        profile: defaultProfile,
        vaults: [defaultVault],
        treeByVaultId: {
            [defaultVault.id]: [{ name: NOTE, path: NOTE, is_directory: false, modified: new Date().toISOString() }],
        },
        fileContentsByVaultId: { [defaultVault.id]: { [NOTE]: '# Test Note' } },
        fileFrontmatterByVaultId: { [defaultVault.id]: { [NOTE]: frontmatter } },
    });
    await page.goto('/');
    await page.getByText(NOTE).click();
    await expandFrontmatter(page);
}

test.describe('Frontmatter panel operations', () => {
    test('shows existing fields in form mode', async ({ page }) => {
        await setup(page, { title: 'My Note', status: 'wip', priority: 'high' });

        await expect(page.locator('.v-expansion-panel-text input[value="title"]')).toBeVisible();
        await expect(page.locator('.v-expansion-panel-text input[value="status"]')).toBeVisible();
        await expect(page.locator('.v-expansion-panel-text input[value="priority"]')).toBeVisible();
    });

    test('marks tab dirty after editing a frontmatter value', async ({ page }) => {
        await setup(page, { status: 'wip' });

        const valueInput = page.locator('.v-expansion-panel-text').getByLabel('Value').first();
        await valueInput.fill('done');
        await valueInput.blur();

        await expect(page.locator('.tab-item')).toContainText('●');
    });

    test('adds a new frontmatter field', async ({ page }) => {
        await setup(page, { existing: 'value' });

        // Click "Add field"
        await page.getByRole('button', { name: 'Add field' }).click();
        await expect(page.getByPlaceholder('field')).toBeVisible();

        await page.getByPlaceholder('field').fill('newkey');
        await page.getByPlaceholder('field').press('Enter');

        // New field appears in the form
        await expect(page.locator('.v-expansion-panel-text input[value="newkey"]')).toBeVisible();
    });

    test('deletes a frontmatter field', async ({ page }) => {
        await setup(page, { title: 'Hello', removable: 'yes' });

        const removableField = page.locator('.v-expansion-panel-text .d-flex.align-center.ga-2').filter({
            has: page.locator('input[value="removable"]'),
        });

        await removableField.getByRole('button').click();

        await expect(page.locator('.v-expansion-panel-text input[value="removable"]')).toHaveCount(0);
    });

    test('switches between Form and Raw modes', async ({ page }) => {
        await setup(page, { status: 'draft' });

        // In Form mode: inputs visible
        await expect(page.locator('.v-expansion-panel-text input[value="status"]')).toBeVisible();

        // Switch to Raw
        await page.locator('.v-expansion-panel-text').getByRole('button', { name: 'Raw' }).click();
        await expect(page.getByRole('textbox', { name: 'Frontmatter (YAML/JSON object)' })).toBeVisible();
        await expect(page.locator('.v-expansion-panel-text input[value="status"]')).not.toBeVisible();

        // Switch back to Form
        await page.locator('.v-expansion-panel-text').getByRole('button', { name: 'Form' }).click();
        await expect(page.locator('.v-expansion-panel-text input[value="status"]')).toBeVisible();
    });

    test('shows empty state when no frontmatter fields', async ({ page }) => {
        await setup(page, {});

        await expect(page.getByText('No frontmatter fields yet')).toBeVisible();
        await expect(page.getByRole('button', { name: 'Add field' })).toBeVisible();
    });

    test('raw mode accepts YAML text and reflects on form mode', async ({ page }) => {
        await setup(page, {});

        await page.locator('.v-expansion-panel-text').getByRole('button', { name: 'Raw' }).click();

        const rawArea = page.getByRole('textbox', { name: 'Frontmatter (YAML/JSON object)' });
        await rawArea.fill('category: programming\nlang: rust');
        await rawArea.blur();

        // Switch to form and verify fields parsed
        await page.locator('.v-expansion-panel-text').getByRole('button', { name: 'Form' }).click();
        await expect(page.locator('.v-expansion-panel-text input[value="category"]')).toBeVisible();
        await expect(page.locator('.v-expansion-panel-text input[value="lang"]')).toBeVisible();
    });
});
