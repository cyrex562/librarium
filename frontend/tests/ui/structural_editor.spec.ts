import { expect, test } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

const characterContent = `---
librarium_type: character
librarium_plugin: worldbuilding
librarium_labels:
  - graphable
name: Aria
age: "28"
occupation: Ranger
---

<!-- librarium:prose:begin -->
A skilled ranger from the northern wilds.
<!-- librarium:prose:end -->
`;

const characterType = {
    id: 'character',
    name: 'Character',
    plugin_id: 'worldbuilding',
    color: '#4A90D9',
    icon: 'mdi-account',
    labels: ['graphable'],
    show_on_create: ['name'],
    display_field: 'name',
    fields: [
        { key: 'name', label: 'Name', field_type: 'string', required: true },
        { key: 'age', label: 'Age', field_type: 'string', required: false },
        { key: 'occupation', label: 'Occupation', field_type: 'string', required: false },
    ],
};

test.describe('Structural editor', () => {
    test('switches to structural mode and renders entity fields', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [
                    { name: 'aria.md', path: 'aria.md', is_directory: false, modified: new Date().toISOString() },
                ],
            },
            fileContentsByVaultId: {
                [defaultVault.id]: { 'aria.md': characterContent },
            },
            entityTypes: [characterType],
        });

        await page.goto('/');

        // Open the file
        await page.getByText('aria.md').click();

        // Wait for file to open in a tab
        await expect(page.locator('.tab-item')).toContainText('aria.md', { timeout: 8000 });

        // Switch to structural mode via the toolbar toggle
        await page.locator('button[title="Structural entity editor"]').click();

        // Structural editor should render the field labels from the schema
        await expect(page.locator('.structural-editor')).toBeVisible();
        await expect(page.getByText('Name')).toBeVisible();
        await expect(page.getByText('Age')).toBeVisible();
        await expect(page.getByText('Occupation')).toBeVisible();
    });

    test('shows error state when file has no librarium_type', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [
                    { name: 'plain.md', path: 'plain.md', is_directory: false, modified: new Date().toISOString() },
                ],
            },
            fileContentsByVaultId: {
                [defaultVault.id]: { 'plain.md': '# Just a plain note\n\nNo frontmatter type here.\n' },
            },
            entityTypes: [characterType],
        });

        await page.goto('/');
        await page.getByText('plain.md').click();
        await expect(page.locator('.tab-item')).toContainText('plain.md', { timeout: 8000 });

        // Switch to structural mode
        await page.locator('button[title="Structural entity editor"]').click();

        // Should show the "no_type" error state
        const structuralEditor = page.locator('.structural-editor');
        await expect(structuralEditor).toBeVisible();
        await expect(structuralEditor.getByRole('heading', { name: 'Not a typed entity' })).toBeVisible();
        await expect(structuralEditor.getByText('librarium_type', { exact: true })).toBeVisible();
    });

    test('structural editor shows prose zone below fields', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [
                    { name: 'aria.md', path: 'aria.md', is_directory: false, modified: new Date().toISOString() },
                ],
            },
            fileContentsByVaultId: {
                [defaultVault.id]: { 'aria.md': characterContent },
            },
            entityTypes: [characterType],
        });

        await page.goto('/');
        await page.getByText('aria.md').click();
        await expect(page.locator('.tab-item')).toContainText('aria.md', { timeout: 8000 });
        await page.locator('button[title="Structural entity editor"]').click();

        const structuralEditor = page.locator('.structural-editor');
        await expect(structuralEditor).toBeVisible();
        await expect(structuralEditor.locator('.prose-zone-wrapper')).toBeVisible();
        await expect(structuralEditor.getByText('A skilled ranger from the northern wilds.')).toBeVisible();
    });
});
