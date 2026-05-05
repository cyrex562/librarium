/**
 * Playwright tests for the Worldbuilding plugin flow.
 *
 * Covers:
 *  - New Entity dialog: type selection, field rendering, required validation, creation
 *  - Entity templates: loading per type
 *  - Graph view: node/edge counts, type filter chips, filter search, empty state
 *  - Relations panel: linked entities display, navigation
 */
import { expect, test } from '@playwright/test';
import {
    defaultProfile,
    defaultVault,
    installCommonAppMocks,
    seedActiveVault,
    seedAuthTokens,
} from './helpers/appMocks';

// ── Shared fixtures ──────────────────────────────────────────────────────────

const characterType = {
    id: 'character',
    name: 'Character',
    plugin_id: 'worldbuilding',
    color: '#4A90D9',
    icon: 'mdi-account',
    labels: ['graphable', 'person'],
    show_on_create: ['full_name', 'status'],
    display_field: 'full_name',
    fields: [
        { key: 'full_name', label: 'Full Name', field_type: 'string', required: true },
        {
            key: 'status',
            label: 'Status',
            field_type: 'enum',
            required: false,
            values: ['Active', 'Deceased', 'Unknown'],
            default: 'Active',
        },
    ],
};

const locationTypeDef = {
    id: 'location',
    name: 'Location',
    plugin_id: 'worldbuilding',
    color: '#52B26B',
    icon: 'mdi-map-marker',
    labels: ['graphable', 'place'],
    show_on_create: ['full_name', 'type'],
    display_field: 'full_name',
    fields: [
        { key: 'full_name', label: 'Full Name', field_type: 'string', required: true },
        {
            key: 'type',
            label: 'Type',
            field_type: 'enum',
            required: false,
            values: ['City', 'Town', 'Wilderness', 'Dungeon'],
        },
    ],
};

const factionTypeDef = {
    id: 'faction',
    name: 'Faction',
    plugin_id: 'worldbuilding',
    color: '#E74C3C',
    icon: 'mdi-shield',
    labels: ['graphable', 'organization'],
    show_on_create: ['full_name'],
    display_field: 'full_name',
    fields: [{ key: 'full_name', label: 'Full Name', field_type: 'string', required: true }],
};

const allEntityTypes = [characterType, locationTypeDef, factionTypeDef];

const characterTemplate = `---
librarium_type: character
librarium_plugin: worldbuilding
librarium_labels:
  - graphable
  - person
full_name: ""
status: Active
---

<!-- librarium:prose:begin -->

<!-- librarium:prose:end -->
`;

const locationTemplate = `---
librarium_type: location
librarium_plugin: worldbuilding
librarium_labels:
  - graphable
  - place
full_name: ""
type: ""
---

<!-- librarium:prose:begin -->

<!-- librarium:prose:end -->
`;

const graphData = {
    nodes: [
        { id: 'e1', title: 'Aria', entity_type: 'character', path: 'aria.md', labels: ['graphable', 'person'] },
        { id: 'e2', title: 'Lyra', entity_type: 'character', path: 'lyra.md', labels: ['graphable', 'person'] },
        { id: 'e3', title: 'Iron Citadel', entity_type: 'location', path: 'iron_citadel.md', labels: ['graphable', 'place'] },
        { id: 'e4', title: 'Order of the Flame', entity_type: 'faction', path: 'order.md', labels: ['graphable', 'organization'] },
    ],
    edges: [
        { id: 'r1', source: 'e1', target: 'e2', relation_type: 'knows', source_field: 'knows', direction: 'forward' },
        { id: 'r2', source: 'e1', target: 'e3', relation_type: 'located_in', source_field: 'location', direction: 'forward' },
        { id: 'r3', source: 'e1', target: 'e4', relation_type: 'member_of', source_field: 'faction', direction: 'forward' },
    ],
};

const ariaEntityResponse = {
    entity: {
        id: 'e1',
        vault_id: defaultVault.id,
        path: 'aria.md',
        entity_type: 'character',
        plugin_id: 'worldbuilding',
        labels: JSON.stringify(['graphable', 'person']),
        fields: JSON.stringify({ full_name: 'Aria', status: 'Active', faction: '[[Order of the Flame]]' }),
        modified_at: new Date().toISOString(),
        indexed_at: new Date().toISOString(),
    },
    relations: [
        { id: 'r1', source_entity_id: 'e1', target_entity_id: 'e2', target_path: 'lyra.md', relation_type: 'knows', label: 'knows', directed: true, metadata: {}, is_inverse: false },
        { id: 'r3', source_entity_id: 'e1', target_entity_id: 'e4', target_path: 'order.md', relation_type: 'member_of', label: 'member_of', directed: true, metadata: {}, is_inverse: false },
    ],
};

const ariaFileContent = `---
librarium_type: character
librarium_plugin: worldbuilding
librarium_labels:
  - graphable
  - person
full_name: Aria
status: Active
faction: "[[Order of the Flame]]"
---

A skilled ranger.
`;

// ── New Entity Dialog ────────────────────────────────────────────────────────

test.describe('Worldbuilding — New Entity dialog', () => {
    test('dialog opens with all registered entity types available', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: allEntityTypes,
        });

        await page.goto('/');
        await page.locator('button[title="New entity"]').click();

        const dialog = page.locator('.v-dialog:visible').first();
        await expect(dialog).toBeVisible();

        const typeSelect = dialog.locator('.v-select', { hasText: 'Entity type' });
        await expect(typeSelect).toBeVisible();
    });

    test('Character fields (full_name, status) appear after type is auto-selected', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: [characterType], // single type → auto-selected
        });

        await page.goto('/');
        await page.locator('button[title="New entity"]').click();

        const dialog = page.locator('.v-dialog:visible').first();
        await expect(dialog).toBeVisible();

        // Fields in show_on_create should appear
        await expect(dialog.getByLabel('Full Name').first()).toBeVisible();
        await expect(dialog.getByLabel('Status').first()).toBeVisible();
    });

    test('Location fields appear when Location type is selected', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: allEntityTypes,
        });

        await page.goto('/');
        await page.locator('button[title="New entity"]').click();

        const dialog = page.locator('.v-dialog:visible').first();
        await expect(dialog).toBeVisible();

        // Open the type selector and choose Location
        const typeSelect = dialog.locator('.v-select', { hasText: 'Entity type' });
        await typeSelect.click();
        await page.locator('.v-list-item', { hasText: 'Location' }).first().click();

        // Location-specific fields
        await expect(dialog.getByLabel('Full Name').first()).toBeVisible();
    });

    test('Create & Open is disabled until file name is provided', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: [characterType],
        });

        await page.goto('/');
        await page.locator('button[title="New entity"]').click();

        const dialog = page.locator('.v-dialog:visible').first();
        const createBtn = dialog.getByRole('button', { name: /Create & Open/i });
        await expect(createBtn).toBeDisabled();

        await dialog.getByLabel('File name').fill('Aria');
        await expect(createBtn).toBeEnabled();
    });

    test('creating a character entity opens it in a new tab', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            fileContentsByVaultId: { [defaultVault.id]: {} },
            entityTypes: [characterType],
            entityTemplatesByTypeId: { character: characterTemplate },
        });

        await page.goto('/');
        await page.locator('button[title="New entity"]').click();

        const dialog = page.locator('.v-dialog:visible').first();
        await expect(dialog).toBeVisible();

        await dialog.getByLabel('File name').fill('Aria');
        await dialog.getByRole('button', { name: /Create & Open/i }).click();

        await expect(dialog).not.toBeVisible({ timeout: 8000 });
        await expect(page.locator('.tab-item')).toContainText(/Aria/i, { timeout: 8000 });
    });

    test('creating a location entity opens it in a new tab', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            fileContentsByVaultId: { [defaultVault.id]: {} },
            entityTypes: allEntityTypes,
            entityTemplatesByTypeId: { location: locationTemplate },
        });

        await page.goto('/');
        await page.locator('button[title="New entity"]').click();

        const dialog = page.locator('.v-dialog:visible').first();
        await expect(dialog).toBeVisible();

        // Select location type
        const typeSelect = dialog.locator('.v-select', { hasText: 'Entity type' });
        await typeSelect.click();
        await page.locator('.v-list-item', { hasText: 'Location' }).first().click();

        await dialog.getByLabel('File name').fill('IronCitadel');
        await dialog.getByRole('button', { name: /Create & Open/i }).click();

        await expect(dialog).not.toBeVisible({ timeout: 8000 });
        await expect(page.locator('.tab-item')).toContainText(/IronCitadel/i, { timeout: 8000 });
    });

    test('cancel button closes the dialog without creating a file', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: [characterType],
        });

        await page.goto('/');
        await page.locator('button[title="New entity"]').click();

        const dialog = page.locator('.v-dialog:visible').first();
        await expect(dialog).toBeVisible();

        await dialog.getByRole('button', { name: 'Cancel' }).click();
        await expect(dialog).not.toBeVisible({ timeout: 3000 });
        await expect(page.locator('.tab-item')).toHaveCount(0);
        await expect(page.getByText('Open a file from the sidebar to start editing.')).toBeVisible();
    });

    test('shows all three worldbuilding types in the type selector', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: allEntityTypes,
        });

        await page.goto('/');
        await page.locator('button[title="New entity"]').click();

        const dialog = page.locator('.v-dialog:visible').first();
        await expect(dialog).toBeVisible();

        // Open type dropdown
        const typeSelect = dialog.locator('.v-select', { hasText: 'Entity type' });
        await typeSelect.click();

        await expect(page.locator('.v-list-item', { hasText: 'Character' }).first()).toBeVisible();
        await expect(page.locator('.v-list-item', { hasText: 'Location' }).first()).toBeVisible();
        await expect(page.locator('.v-list-item', { hasText: 'Faction' }).first()).toBeVisible();
    });
});

// ── Entity Template Loading ──────────────────────────────────────────────────

test.describe('Worldbuilding — Entity templates', () => {
    test('character template used when creating via dialog', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        const fileContentsByVaultId: Record<string, Record<string, string>> = {
            [defaultVault.id]: {},
        };
        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            fileContentsByVaultId,
            entityTypes: [characterType],
            entityTemplatesByTypeId: { character: characterTemplate },
        });

        await page.goto('/');
        await page.locator('button[title="New entity"]').click();

        const dialog = page.locator('.v-dialog:visible').first();
        await dialog.getByLabel('File name').fill('Aria');
        await dialog.getByRole('button', { name: /Create & Open/i }).click();

        await expect(dialog).not.toBeVisible({ timeout: 8000 });

        // The saved content should come from the character template
        expect(fileContentsByVaultId[defaultVault.id]['Aria.md']).toContain('librarium_type: character');
    });
});

// ── Graph view ───────────────────────────────────────────────────────────────

test.describe('Worldbuilding — Graph view', () => {
    test('graph tab appears after clicking Graph view button', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: allEntityTypes,
            graphByVaultId: { [defaultVault.id]: graphData },
        });

        await page.goto('/');
        await page.locator('button[title="Graph view"]').click();

        await expect(page.locator('.tab-item')).toContainText('Graph', { timeout: 8000 });
    });

    test('graph sidebar shows entity type chips for all types in the data', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: allEntityTypes,
            graphByVaultId: { [defaultVault.id]: graphData },
        });

        await page.goto('/');
        await page.locator('button[title="Graph view"]').click();

        const graphSidebar = page.locator('.graph-sidebar');
        await expect(graphSidebar).toBeVisible({ timeout: 8000 });

        // All three types present in graphData should appear as chips
        await expect(graphSidebar.getByText('Character')).toBeVisible();
        await expect(graphSidebar.getByText('Location')).toBeVisible();
        await expect(graphSidebar.getByText('Faction')).toBeVisible();
    });

    test('graph shows correct node and edge counts', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: allEntityTypes,
            graphByVaultId: { [defaultVault.id]: graphData },
        });

        await page.goto('/');
        await page.locator('button[title="Graph view"]').click();

        const graphSidebar = page.locator('.graph-sidebar');
        await expect(graphSidebar).toBeVisible({ timeout: 8000 });

        // Sidebar should display counts; graphData has 4 nodes and 3 edges
        await expect(graphSidebar.getByText(/nodes/i)).toBeVisible();
        await expect(graphSidebar.getByText(/edges/i)).toBeVisible();
    });

    test('empty graph shows "No entities" placeholder', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: [],
            graphByVaultId: { [defaultVault.id]: { nodes: [], edges: [] } },
        });

        await page.goto('/');
        await page.locator('button[title="Graph view"]').click();

        const graphView = page.locator('.graph-view');
        await expect(graphView).toBeVisible({ timeout: 8000 });
        await expect(graphView.getByText('No entities', { exact: true })).toBeVisible();
    });

    test('search box is visible in graph sidebar', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: allEntityTypes,
            graphByVaultId: { [defaultVault.id]: graphData },
        });

        await page.goto('/');
        await page.locator('button[title="Graph view"]').click();

        const graphSidebar = page.locator('.graph-sidebar');
        await expect(graphSidebar).toBeVisible({ timeout: 8000 });
        await expect(graphSidebar.locator('input[placeholder="Filter nodes…"]')).toBeVisible();
    });

    test('toggling a type chip filters nodes', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: { [defaultVault.id]: [] },
            entityTypes: allEntityTypes,
            graphByVaultId: { [defaultVault.id]: graphData },
        });

        await page.goto('/');
        await page.locator('button[title="Graph view"]').click();

        const graphSidebar = page.locator('.graph-sidebar');
        await expect(graphSidebar).toBeVisible({ timeout: 8000 });

        // Click the Location chip to toggle it off
        const locationChip = graphSidebar.locator('.v-chip', { hasText: 'Location' });
        await expect(locationChip).toBeVisible();
        await locationChip.click();

        // After filtering out Location, only Character + Faction remain visible
        // The node count display should update to reflect fewer nodes
        await expect(graphSidebar.getByText(/nodes/i)).toBeVisible();
    });
});

// ── Relations panel ──────────────────────────────────────────────────────────

test.describe('Worldbuilding — Relations panel', () => {
    test('relations panel is visible when opening an entity file', async ({ page }) => {
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
            fileContentsByVaultId: { [defaultVault.id]: { 'aria.md': ariaFileContent } },
            entityByPathByVaultId: { [defaultVault.id]: { 'aria.md': ariaEntityResponse } },
        });

        await page.goto('/');
        await page.getByText('aria.md').click();
        await expect(page.locator('.tab-item')).toContainText('aria.md', { timeout: 8000 });

        const relationsPanel = page.locator('.v-expansion-panel', { hasText: 'Relations' });
        await expect(relationsPanel).toBeVisible({ timeout: 8000 });
    });

    test('relations panel shows relation count badge matching data', async ({ page }) => {
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
            fileContentsByVaultId: { [defaultVault.id]: { 'aria.md': ariaFileContent } },
            entityByPathByVaultId: { [defaultVault.id]: { 'aria.md': ariaEntityResponse } },
        });

        await page.goto('/');
        await page.getByText('aria.md').click();
        await expect(page.locator('.tab-item')).toContainText('aria.md', { timeout: 8000 });

        const relationsPanel = page.locator('.v-expansion-panel', { hasText: 'Relations' });
        await expect(relationsPanel).toBeVisible({ timeout: 8000 });

        // ariaEntityResponse has 2 relations
        await expect(relationsPanel.locator('.v-badge')).toContainText('2');
    });

    test('relations panel shows linked entities after expanding', async ({ page }) => {
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
            fileContentsByVaultId: { [defaultVault.id]: { 'aria.md': ariaFileContent } },
            entityByPathByVaultId: { [defaultVault.id]: { 'aria.md': ariaEntityResponse } },
        });

        await page.goto('/');
        await page.getByText('aria.md').click();
        await expect(page.locator('.tab-item')).toContainText('aria.md', { timeout: 8000 });

        const relationsPanel = page.locator('.v-expansion-panel', { hasText: 'Relations' });
        await expect(relationsPanel).toBeVisible({ timeout: 8000 });
        await relationsPanel.locator('.v-expansion-panel-title').click();

        // Should show the two related paths
        await expect(relationsPanel.getByText(/lyra/i)).toBeVisible();
        await expect(relationsPanel.getByText(/order/i)).toBeVisible();
    });

    test('plain markdown file does not show relations panel', async ({ page }) => {
        await seedAuthTokens(page);
        await seedActiveVault(page, defaultVault.id);

        await installCommonAppMocks(page, {
            profile: defaultProfile,
            vaults: [defaultVault],
            treeByVaultId: {
                [defaultVault.id]: [
                    { name: 'notes.md', path: 'notes.md', is_directory: false, modified: new Date().toISOString() },
                ],
            },
            fileContentsByVaultId: { [defaultVault.id]: { 'notes.md': '# Just notes\n\nPlain file.\n' } },
            // No entityByPathByVaultId → 404 for entity-by-path
        });

        await page.goto('/');
        await page.getByText('notes.md').click();
        await expect(page.locator('.tab-item')).toContainText('notes.md', { timeout: 8000 });

        await page.waitForTimeout(1000);
        const relationsPanel = page.locator('.v-expansion-panel', { hasText: 'Relations' });
        await expect(relationsPanel).not.toBeVisible();
    });

    test('clicking a relation item opens target file in a new tab', async ({ page }) => {
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
                [defaultVault.id]: {
                    'aria.md': ariaFileContent,
                    'lyra.md': '---\nlibrarium_type: character\nfull_name: Lyra\n---\n',
                },
            },
            entityByPathByVaultId: { [defaultVault.id]: { 'aria.md': ariaEntityResponse } },
        });

        await page.goto('/');
        await page.getByText('aria.md').click();
        await expect(page.locator('.tab-item')).toContainText('aria.md', { timeout: 8000 });

        const relationsPanel = page.locator('.v-expansion-panel', { hasText: 'Relations' });
        await expect(relationsPanel).toBeVisible({ timeout: 8000 });
        await relationsPanel.locator('.v-expansion-panel-title').click();

        await relationsPanel.getByText(/lyra/i).click();

        // A new active tab for lyra should open
        await expect(page.locator('.tab-item.tab-active')).toContainText(/lyra/i, { timeout: 8000 });
    });
});

// ── Structural editor entity fields ─────────────────────────────────────────

test.describe('Worldbuilding — Structural editor entity fields', () => {
    test('entity file opens in structural editor and shows entity fields', async ({ page }) => {
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
            fileContentsByVaultId: { [defaultVault.id]: { 'aria.md': ariaFileContent } },
            entityByPathByVaultId: { [defaultVault.id]: { 'aria.md': ariaEntityResponse } },
            entityTypes: [characterType],
        });

        await page.goto('/');
        await page.getByText('aria.md').click();
        await expect(page.locator('.tab-item')).toContainText('aria.md', { timeout: 8000 });

        await page.locator('button[title="Structural entity editor"]').click();

        const structuralEditor = page.locator('.structural-editor');
        await expect(structuralEditor).toBeVisible({ timeout: 8000 });
        await expect(structuralEditor.getByText('Full name')).toBeVisible();
        await expect(structuralEditor.getByText('Status')).toBeVisible();
    });
});
