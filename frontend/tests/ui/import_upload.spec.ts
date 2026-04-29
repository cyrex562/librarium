import { expect, test } from '@playwright/test';
import { defaultProfile, defaultVault, installCommonAppMocks, seedActiveVault, seedAuthTokens } from './helpers/appMocks';

test.describe('Vault import uploads', () => {
    test('queues multiple files and imports them through the dialog', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [],
            },
        });

        await page.goto('/');
        await page.locator('button[title="Import files or folders"]').click();

        await page.setInputFiles('[data-testid="import-files-input"]', [
            {
                name: 'alpha.md',
                mimeType: 'text/markdown',
                buffer: Buffer.from('# Alpha'),
            },
            {
                name: 'beta.txt',
                mimeType: 'text/plain',
                buffer: Buffer.from('Beta'),
            },
        ]);

        await expect(page.getByText('2 queued')).toBeVisible();
        await expect(page.getByText('alpha.md')).toBeVisible();
        await expect(page.getByText('beta.txt')).toBeVisible();

        await page.getByRole('button', { name: 'Import 2' }).click();

        await expect(page.getByText('Imported 2 files successfully.')).toBeVisible();
        await expect(page.locator('.file-tree-node', { hasText: 'alpha.md' })).toBeVisible();
        await expect(page.locator('.file-tree-node', { hasText: 'beta.txt' })).toBeVisible();
    });

    test('opens import dialog with folder target when dropping files on a tree folder', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [
                    {
                        name: 'Inbox',
                        path: 'Inbox',
                        is_directory: true,
                        modified: new Date().toISOString(),
                        children: [],
                    },
                ],
            },
        });

        await page.goto('/');
        const folderNode = page.locator('.file-tree-node', { hasText: 'Inbox' }).first();

        await folderNode.dispatchEvent('drop', {
            dataTransfer: await page.evaluateHandle(() => {
                const data = new DataTransfer();
                data.items.add(new File(['hello'], 'dropped.md', { type: 'text/markdown' }));
                return data;
            }),
        });

        await expect(page.getByText('Import files and folders')).toBeVisible();
        await expect(page.getByRole('combobox', { name: 'Target folder inside vault' })).toHaveValue('Inbox');
        await expect(page.getByText('dropped.md')).toBeVisible();
    });

    test('selects import target from folder picker and autocomplete', async ({ page }) => {
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
                                name: 'Drafts',
                                path: 'Projects/Drafts',
                                is_directory: true,
                                modified: new Date().toISOString(),
                                children: [],
                            },
                        ],
                    },
                    {
                        name: 'Archive',
                        path: 'Archive',
                        is_directory: true,
                        modified: new Date().toISOString(),
                        children: [],
                    },
                ],
            },
        });

        await page.goto('/');
        await page.locator('button[title="Import files or folders"]').click();

        const targetInput = page.getByRole('combobox', { name: 'Target folder inside vault' });
        await page.getByTestId('import-folder-picker').getByText('Projects/Drafts').click();
        await expect(targetInput).toHaveValue('Projects/Drafts');

        await targetInput.fill('Arc');
        await page.getByText('Archive').last().click();
        await expect(targetInput).toHaveValue('Archive');
    });

    test('queues files dropped on the dialog card (outside the dashed dropzone)', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [],
            },
        });

        await page.goto('/');
        await page.locator('button[title="Import files or folders"]').click();
        await expect(page.getByText('Import files and folders')).toBeVisible();

        const card = page.locator('.v-dialog .v-card').first();
        await card.dispatchEvent('drop', {
            dataTransfer: await page.evaluateHandle(() => {
                const data = new DataTransfer();
                data.items.add(new File(['hi'], 'card-drop.md', { type: 'text/markdown' }));
                return data;
            }),
        });

        await expect(page.getByText('1 queued')).toBeVisible();
        await expect(page.getByText('card-drop.md')).toBeVisible();
    });

    test('persistent dialog: outside click does not dismiss; Esc does', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [],
            },
        });

        await page.goto('/');
        await page.locator('button[title="Import files or folders"]').click();
        await expect(page.getByText('Import files and folders')).toBeVisible();

        // Click on the scrim (outside the card) – dialog should stay open.
        const scrim = page.locator('.v-overlay__scrim').first();
        await scrim.click({ force: true });
        await expect(page.getByText('Import files and folders')).toBeVisible();

        // Esc closes.
        await page.keyboard.press('Escape');
        await expect(page.getByText('Import files and folders')).toBeHidden();
    });

    test('Cancel transfer button confirms then aborts the import', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [],
            },
        });

        // Stall the upload session creation so import stays in flight.
        let releaseSession: (() => void) | null = null;
        const sessionStalled = new Promise<void>((resolve) => {
            releaseSession = resolve;
        });
        await page.route(/.*\/api\/vaults\/[^/]+\/upload-sessions$/, async (route) => {
            await sessionStalled;
            await route.fallback();
        });

        await page.goto('/');
        await page.locator('button[title="Import files or folders"]').click();

        await page.setInputFiles('[data-testid="import-files-input"]', {
            name: 'slow.md',
            mimeType: 'text/markdown',
            buffer: Buffer.from('hi'),
        });

        const dialogMessages: string[] = [];
        page.on('dialog', async (d) => {
            dialogMessages.push(d.message());
            await d.accept();
        });

        await page.getByRole('button', { name: 'Import 1' }).click();
        await expect(page.getByRole('button', { name: 'Cancel transfer' })).toBeVisible();

        await page.getByRole('button', { name: 'Cancel transfer' }).click();
        releaseSession?.();

        await expect(page.getByText('Import canceled.')).toBeVisible();
        expect(dialogMessages.some((m) => /cancel/i.test(m))).toBe(true);
    });

    test('Esc during an import triggers the cancel-transfer confirm', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [],
            },
        });

        let releaseSession: (() => void) | null = null;
        const sessionStalled = new Promise<void>((resolve) => {
            releaseSession = resolve;
        });
        await page.route(/.*\/api\/vaults\/[^/]+\/upload-sessions$/, async (route) => {
            await sessionStalled;
            await route.fallback();
        });

        await page.goto('/');
        await page.locator('button[title="Import files or folders"]').click();
        await page.setInputFiles('[data-testid="import-files-input"]', {
            name: 'slow.md',
            mimeType: 'text/markdown',
            buffer: Buffer.from('hi'),
        });

        page.on('dialog', async (d) => {
            await d.accept();
        });

        await page.getByRole('button', { name: 'Import 1' }).click();
        await expect(page.getByRole('button', { name: 'Cancel transfer' })).toBeVisible();

        await page.keyboard.press('Escape');
        releaseSession?.();

        await expect(page.getByText('Import canceled.')).toBeVisible();
        // Dialog stays open so the user sees the canceled state.
        await expect(page.getByText('Import files and folders')).toBeVisible();
    });

    test('queues files dropped onto the dropzone inside the dialog', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [],
            },
        });

        await page.goto('/');
        await page.locator('button[title="Import files or folders"]').click();
        await expect(page.getByText('Import files and folders')).toBeVisible();

        const dropzone = page.getByTestId('import-dropzone');
        await dropzone.dispatchEvent('drop', {
            dataTransfer: await page.evaluateHandle(() => {
                const data = new DataTransfer();
                data.items.add(new File(['hello'], 'dragged.md', { type: 'text/markdown' }));
                return data;
            }),
        });

        await expect(page.getByText('1 queued')).toBeVisible();
        await expect(page.getByText('dragged.md')).toBeVisible();
    });

    test('imports into a newly typed target folder', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);
        let finishPayload: { filename: string; path?: string } | null = null;

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [],
            },
        });
        await page.route(/.*\/api\/vaults\/[^/]+\/upload-sessions\/[^/]+\/finish$/, async (route) => {
            finishPayload = route.request().postDataJSON() as { filename: string; path?: string };
            await route.fallback();
        });

        await page.goto('/');
        await page.locator('button[title="Import files or folders"]').click();

        await page.getByRole('combobox', { name: 'Target folder inside vault' }).fill('New Folder/Subfolder');
        await page.setInputFiles('[data-testid="import-files-input"]', {
            name: 'note.md',
            mimeType: 'text/markdown',
            buffer: Buffer.from('# Note'),
        });

        await page.getByRole('button', { name: 'Import 1' }).click();

        await expect(page.getByText('Imported 1 file successfully.')).toBeVisible();
        expect(finishPayload).toEqual(expect.objectContaining({ filename: 'note.md', path: 'New Folder/Subfolder' }));
        await expect(page.locator('.file-tree-node', { hasText: 'New Folder' })).toBeVisible();
        await expect(page.locator('.file-tree-node', { hasText: 'Subfolder' })).toBeVisible();
    });
});
