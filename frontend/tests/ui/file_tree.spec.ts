import { expect, test } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

test.describe('File tree navigation', () => {
    test('renders nested structure and toggles folder expansion', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [
                    {
                        name: 'folder_b',
                        path: 'folder_b',
                        is_directory: true,
                        modified: new Date().toISOString(),
                        children: [
                            {
                                name: 'nested_note.md',
                                path: 'folder_b/nested_note.md',
                                is_directory: false,
                                modified: new Date().toISOString(),
                            },
                        ],
                    },
                    {
                        name: 'root_note.md',
                        path: 'root_note.md',
                        is_directory: false,
                        modified: new Date().toISOString(),
                    },
                ],
            },
            fileContentsByVaultId: {
                [defaultVault.id]: {
                    'folder_b/nested_note.md': '# Nested Note',
                },
            },
        });

        await page.goto('/');
        await expect(page.getByText('root_note.md')).toBeVisible();
        // Folders start collapsed (FileTreeNode.vue: `const expanded = ref(false)`),
        // so the nested child is hidden until the folder is opened. This test used
        // to assume the opposite and asserted the child was visible on load.
        await expect(page.getByText('nested_note.md')).not.toBeVisible();

        const folderRow = page.locator('.file-tree-node', { hasText: 'folder_b' }).first();
        await folderRow.click();
        await expect(page.getByText('nested_note.md')).toBeVisible();

        await folderRow.click();
        await expect(page.getByText('nested_note.md')).not.toBeVisible();
    });

    test('collapses all open folders from the sidebar action', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [
                    {
                        name: 'Projects',
                        path: 'Projects',
                        is_directory: true,
                        modified: new Date().toISOString(),
                        children: [
                            {
                                name: 'Roadmap.md',
                                path: 'Projects/Roadmap.md',
                                is_directory: false,
                                modified: new Date().toISOString(),
                            },
                        ],
                    },
                ],
            },
        });

        await page.goto('/');
        // Folders start collapsed, so open one first — otherwise "collapse all"
        // is asserting against a tree that was never expanded.
        await expect(page.getByText('Projects')).toBeVisible();
        await page.locator('.file-tree-node', { hasText: 'Projects' }).first().click();
        await expect(page.getByText('Roadmap.md')).toBeVisible();

        await page.locator('button[title="Collapse all folders"]').click();

        await expect(page.getByText('Projects')).toBeVisible();
        await expect(page.getByText('Roadmap.md')).not.toBeVisible();
    });
});
