import type { Page } from '@playwright/test';

type UserProfile = {
    id: string;
    username: string;
    is_admin: boolean;
    must_change_password: boolean;
    groups: Array<{ id: string; name: string; created_at: string }>;
    auth_method: string;
};

type MockOptions = {
    profile?: UserProfile;
    vaults?: Array<{ id: string; name: string; path: string; path_exists: boolean; created_at: string; updated_at: string }>;
    treeByVaultId?: Record<string, unknown[]>;
    fileContentsByVaultId?: Record<string, Record<string, string>>;
    fileFrontmatterByVaultId?: Record<string, Record<string, Record<string, unknown>>>;
    plugins?: Array<{ id: string; name: string; description: string; enabled: boolean }>;
    searchResults?: Array<{ path: string; title: string; matches: Array<{ line_number: number; line_text: string; match_start: number; match_end: number }>; score: number }>;
    /** Bookmarks keyed by vault id */
    bookmarksByVaultId?: Record<string, Array<{ id: string; path: string; title: string }>>;
    /** Tags keyed by vault id */
    tagsByVaultId?: Record<string, Array<{ tag: string; count: number }>>;
    /** Backlinks keyed by vault id, then by file path */
    backlinksByVaultId?: Record<string, Array<{ path: string; title: string }>>;
    /** ML outline mock result */
    outlineResult?: { file_path?: string; summary: string; sections: Array<{ line_number: number; level: number; title: string; summary: string }>; generated_at?: string };
    /** ML suggestions mock result */
    suggestionsResult?: {
        file_path?: string;
        suggestions: Array<{
            id: string;
            kind: 'tag' | 'category' | 'move_to_folder';
            confidence: number;
            rationale: string;
            tag?: string;
            category?: string;
            target_folder?: string;
        }>;
        existing_tags?: string[];
        generated_at?: string;
    };
    /** Entity types returned from /api/plugins/entity-types */
    entityTypes?: Array<{ id: string; name: string; plugin_id: string; color?: string; icon?: string; fields: unknown[]; labels?: string[]; show_on_create?: string[]; display_field?: string }>;
    /** Relation types returned from /api/plugins/relation-types */
    relationTypes?: Array<{ id: string; name: string; plugin_id: string; source_types: string[]; target_types: string[] }>;
    /** Graph data keyed by vault id */
    graphByVaultId?: Record<string, { nodes: unknown[]; edges: unknown[] }>;
    /** Entity-by-path responses keyed by vault id, then file path */
    entityByPathByVaultId?: Record<string, Record<string, { entity: unknown | null; relations: unknown[] }>>;
    /** Entity-type template content keyed by type id */
    entityTemplatesByTypeId?: Record<string, string>;
    /** Admin entity index stats (requires admin profile) */
    entityIndexStats?: { vaults: Array<{ vault_id: string; vault_name: string; entity_count: number }> };
};

export const defaultProfile: UserProfile = {
    id: 'u1',
    username: 'alice',
    is_admin: false,
    must_change_password: false,
    groups: [],
    auth_method: 'password',
};

export const defaultVault = {
    id: 'v1',
    name: 'Demo Vault',
    path: 'C:/vaults/demo',
    path_exists: true,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
};

export async function seedAuthTokens(page: Page, access = 'access-token', refresh = 'refresh-token') {
    await page.addInitScript(
        ({ accessToken, refreshToken }) => {
            localStorage.setItem('obsidian_access_token', accessToken);
            localStorage.setItem('obsidian_refresh_token', refreshToken);
            localStorage.setItem('obsidian_token_expires_at', String(Date.now() + 60 * 60 * 1000));
        },
        { accessToken: access, refreshToken: refresh },
    );
}

export async function seedActiveVault(page: Page, vaultId: string) {
    await page.addInitScript((id) => {
        localStorage.setItem('obsidian_active_vault', id);
    }, vaultId);
}

export async function installCommonAppMocks(page: Page, options: MockOptions = {}) {
    const profile = options.profile ?? defaultProfile;
    const vaults = [...(options.vaults ?? [])];
    const treeByVaultId = options.treeByVaultId ?? {};
    const fileContentsByVaultId = options.fileContentsByVaultId ?? {};
    const fileFrontmatterByVaultId = options.fileFrontmatterByVaultId ?? {};
    const plugins = options.plugins ?? [];
    const searchResults = options.searchResults ?? [];
    const bookmarksByVaultId: Record<string, Array<{ id: string; path: string; title: string }>> = {};
    for (const [vid, bms] of Object.entries(options.bookmarksByVaultId ?? {})) {
        bookmarksByVaultId[vid] = [...bms];
    }
    const tagsByVaultId = options.tagsByVaultId ?? {};
    const backlinksByVaultId = options.backlinksByVaultId ?? {};
    const uploadSessions = new Map<string, { filename: string; path: string; uploadedBytes: number; totalSize: number }>();
    const entityTypes = options.entityTypes ?? [];
    const relationTypes = options.relationTypes ?? [];
    const graphByVaultId = options.graphByVaultId ?? {};
    const entityByPathByVaultId = options.entityByPathByVaultId ?? {};
    const entityTemplatesByTypeId = options.entityTemplatesByTypeId ?? {};
    const entityIndexStats = options.entityIndexStats ?? { vaults: [] };

    function ensureDirectoryNode(vaultId: string, path: string) {
        if (!path) return;
        const segments = path.split('/').filter(Boolean);
        const root = (treeByVaultId[vaultId] as Array<any> | undefined) ?? [];
        treeByVaultId[vaultId] = root;

        let level = root;
        let currentPath = '';
        for (const segment of segments) {
            currentPath = currentPath ? `${currentPath}/${segment}` : segment;
            let node = level.find((entry) => entry.path === currentPath && entry.is_directory);
            if (!node) {
                node = {
                    name: segment,
                    path: currentPath,
                    is_directory: true,
                    modified: new Date().toISOString(),
                    children: [],
                };
                level.push(node);
            }
            node.children ??= [];
            level = node.children;
        }
    }

    function upsertFileNode(vaultId: string, filePath: string, size: number) {
        const normalized = filePath.split('/').filter(Boolean).join('/');
        const parentPath = normalized.includes('/') ? normalized.slice(0, normalized.lastIndexOf('/')) : '';
        ensureDirectoryNode(vaultId, parentPath);

        const root = (treeByVaultId[vaultId] as Array<any> | undefined) ?? [];
        treeByVaultId[vaultId] = root;

        let level = root;
        if (parentPath) {
            const segments = parentPath.split('/');
            let current = '';
            for (const segment of segments) {
                current = current ? `${current}/${segment}` : segment;
                const dirNode = level.find((entry) => entry.path === current && entry.is_directory);
                level = (dirNode?.children as Array<any> | undefined) ?? [];
            }
        }

        const fileName = normalized.split('/').pop() ?? normalized;
        const existingIndex = level.findIndex((entry) => entry.path === normalized && !entry.is_directory);
        const fileNode = {
            name: fileName,
            path: normalized,
            is_directory: false,
            modified: new Date().toISOString(),
            size,
        };

        if (existingIndex >= 0) {
            level.splice(existingIndex, 1, fileNode);
        } else {
            level.push(fileNode);
        }
    }

    const prefs = {
        theme: 'dark',
        editor_mode: 'side_by_side',
        font_size: 14,
        window_layout: null,
    };

    await page.route('**/api/auth/refresh', async (route) => {
        await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ access_token: 'access-token', refresh_token: 'refresh-token', expires_in: 3600 }),
        });
    });

    await page.route('**/api/auth/me', async (route) => {
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(profile) });
    });

    await page.route('**/api/vaults', async (route) => {
        const method = route.request().method();
        if (method === 'GET') {
            await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(vaults) });
            return;
        }

        if (method === 'POST') {
            const payload = route.request().postDataJSON() as { name: string; path?: string };
            const slug = payload.name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
            const created = {
                id: `v-${vaults.length + 1}`,
                name: payload.name,
                path: payload.path ?? `C:/vaults/${slug || `vault-${vaults.length + 1}`}`,
                path_exists: true,
                created_at: new Date().toISOString(),
                updated_at: new Date().toISOString(),
            };
            vaults.push(created);
            treeByVaultId[created.id] = [];
            fileContentsByVaultId[created.id] = {};
            fileFrontmatterByVaultId[created.id] = {};
            await route.fulfill({ status: 201, contentType: 'application/json', body: JSON.stringify(created) });
            return;
        }

        await route.continue();
    });

    await page.route(/.*\/api\/vaults\/[^/]+$/, async (route) => {
        if (route.request().method() !== 'DELETE') {
            await route.continue();
            return;
        }

        const match = route.request().url().match(/\/api\/vaults\/([^/]+)$/);
        const vaultId = match?.[1] ?? '';
        const idx = vaults.findIndex((v) => v.id === vaultId);
        if (idx >= 0) vaults.splice(idx, 1);
        delete treeByVaultId[vaultId];
        delete fileContentsByVaultId[vaultId];
        delete fileFrontmatterByVaultId[vaultId];
        await route.fulfill({ status: 204, body: '' });
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/files$/, async (route) => {
        const method = route.request().method();
        const match = route.request().url().match(/\/api\/vaults\/([^/]+)\/files$/);
        const vaultId = match?.[1] ?? '';

        if (method === 'GET') {
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify(treeByVaultId[vaultId] ?? []),
            });
            return;
        }

        if (method === 'POST') {
            const payload = route.request().postDataJSON() as { path: string; content?: string };
            const existingTree = (treeByVaultId[vaultId] as Array<any> | undefined) ?? [];
            existingTree.push({
                name: payload.path.split('/').pop() ?? payload.path,
                path: payload.path,
                is_directory: false,
                modified: new Date().toISOString(),
            });
            treeByVaultId[vaultId] = existingTree;

            const contentMap = (fileContentsByVaultId[vaultId] ??= {});
            contentMap[payload.path] = payload.content ?? '';
            const fmMap = (fileFrontmatterByVaultId[vaultId] ??= {});
            fmMap[payload.path] = {};

            await route.fulfill({
                status: 201,
                contentType: 'application/json',
                body: JSON.stringify({
                    path: payload.path,
                    content: contentMap[payload.path],
                    modified: new Date().toISOString(),
                    frontmatter: fmMap[payload.path],
                }),
            });
            return;
        }

        await route.continue();
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/files\/.+/, async (route) => {
        const method = route.request().method();
        const match = route.request().url().match(/\/api\/vaults\/([^/]+)\/files\/(.+)$/);
        const vaultId = match?.[1] ?? '';
        const rawPath = match?.[2] ?? '';
        const filePath = decodeURIComponent(rawPath);

        const contentMap = (fileContentsByVaultId[vaultId] ??= {});
        const fmMap = (fileFrontmatterByVaultId[vaultId] ??= {});

        if (method === 'GET') {
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify({
                    path: filePath,
                    content: contentMap[filePath] ?? `# ${filePath}`,
                    modified: new Date().toISOString(),
                    frontmatter: fmMap[filePath] ?? {},
                }),
            });
            return;
        }

        if (method === 'PUT') {
            const payload = route.request().postDataJSON() as { content: string };
            contentMap[filePath] = payload.content;
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify({
                    path: filePath,
                    content: payload.content,
                    modified: new Date().toISOString(),
                    frontmatter: fmMap[filePath] ?? {},
                }),
            });
            return;
        }

        if (method === 'DELETE') {
            delete contentMap[filePath];
            delete fmMap[filePath];
            const existingTree = (treeByVaultId[vaultId] as Array<any> | undefined) ?? [];
            treeByVaultId[vaultId] = existingTree.filter((n) => n.path !== filePath);
            await route.fulfill({ status: 204, body: '' });
            return;
        }

        await route.continue();
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/directories$/, async (route) => {
        if (route.request().method() !== 'POST') {
            await route.continue();
            return;
        }

        const match = route.request().url().match(/\/api\/vaults\/([^/]+)\/directories$/);
        const vaultId = match?.[1] ?? '';
        const payload = route.request().postDataJSON() as { path: string };
        ensureDirectoryNode(vaultId, payload.path);

        await route.fulfill({ status: 204, body: '' });
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/rename$/, async (route) => {
        if (route.request().method() !== 'POST') {
            await route.continue();
            return;
        }

        const match = route.request().url().match(/\/api\/vaults\/([^/]+)\/rename$/);
        const vaultId = match?.[1] ?? '';
        const payload = route.request().postDataJSON() as { from: string; to: string };
        const contentMap = (fileContentsByVaultId[vaultId] ??= {});
        const fmMap = (fileFrontmatterByVaultId[vaultId] ??= {});
        const existingTree = (treeByVaultId[vaultId] as Array<any> | undefined) ?? [];

        if (payload.from in contentMap) {
            contentMap[payload.to] = contentMap[payload.from];
            delete contentMap[payload.from];
        }

        if (payload.from in fmMap) {
            fmMap[payload.to] = fmMap[payload.from];
            delete fmMap[payload.from];
        }

        treeByVaultId[vaultId] = existingTree.map((n) => {
            if (n.path !== payload.from) return n;
            return {
                ...n,
                name: payload.to.split('/').pop() ?? payload.to,
                path: payload.to,
                modified: new Date().toISOString(),
            };
        });

        await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ new_path: payload.to }),
        });
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/render$/, async (route) => {
        const payload = route.request().postDataJSON() as { content: string };
        const content = payload.content ?? '';
        const withImageEmbeds = content.replace(/!\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_, path: string, alt?: string) => {
            const safePath = path.trim();
            const safeAlt = (alt ?? path).trim();
            return `<img class="wiki-embed" data-original-link="${safePath}" alt="${safeAlt}" src="/api/raw/${safePath}" />`;
        });
        const withWikiLinks = withImageEmbeds.replace(/\[\[([^\]]+)\]\]/g, (_, target: string) => {
            const safeTarget = target.trim();
            return `<a class="wiki-link" data-original-link="${safeTarget}" href="${safeTarget}">${safeTarget}</a>`;
        });

        await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify(`<p>${withWikiLinks.replace(/\n/g, '<br/>')}</p>`),
        });
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/resolve-link$/, async (route) => {
        const payload = route.request().postDataJSON() as { link: string };
        await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({
                path: `${payload.link.replace(/\.md$/i, '')}.md`,
                exists: true,
                ambiguous: false,
                alternatives: [],
            }),
        });
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/reindex$/, async (route) => {
        await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ indexed_files: 1 }),
        });
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/random$/, async (route) => {
        await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ path: 'Random.md' }),
        });
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/daily$/, async (route) => {
        await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({
                path: 'Daily/2026-03-13.md',
                content: '# Daily',
                modified: new Date().toISOString(),
                frontmatter: {},
            }),
        });
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/recent$/, async (route) => {
        const method = route.request().method();
        if (method === 'GET') {
            await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) });
            return;
        }
        if (method === 'POST') {
            await route.fulfill({ status: 204, body: '' });
            return;
        }
        await route.continue();
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/upload-sessions$/, async (route) => {
        if (route.request().method() !== 'POST') {
            await route.continue();
            return;
        }

        const payload = route.request().postDataJSON() as { filename: string; path?: string; total_size?: number };
        const sessionId = `session-${uploadSessions.size + 1}`;
        uploadSessions.set(sessionId, {
            filename: payload.filename,
            path: payload.path ?? '',
            uploadedBytes: 0,
            totalSize: payload.total_size ?? 0,
        });

        await route.fulfill({
            status: 201,
            contentType: 'application/json',
            body: JSON.stringify({ session_id: sessionId, uploaded_bytes: 0, total_size: payload.total_size ?? 0 }),
        });
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/upload-sessions\/[^/]+$/, async (route) => {
        const match = route.request().url().match(/\/api\/vaults\/([^/]+)\/upload-sessions\/([^/]+)$/);
        const sessionId = match?.[2] ?? '';
        const session = uploadSessions.get(sessionId);

        if (!session) {
            await route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ error: 'Session not found' }) });
            return;
        }

        if (route.request().method() === 'GET') {
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify({ session_id: sessionId, uploaded_bytes: session.uploadedBytes, total_size: session.totalSize }),
            });
            return;
        }

        if (route.request().method() === 'PUT') {
            const body = route.request().postDataBuffer() ?? Buffer.alloc(0);
            session.uploadedBytes += body.length;
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify({ uploaded_bytes: session.uploadedBytes }),
            });
            return;
        }

        await route.continue();
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/upload-sessions\/[^/]+\/finish$/, async (route) => {
        if (route.request().method() !== 'POST') {
            await route.continue();
            return;
        }

        const match = route.request().url().match(/\/api\/vaults\/([^/]+)\/upload-sessions\/([^/]+)\/finish$/);
        const vaultId = match?.[1] ?? '';
        const sessionId = match?.[2] ?? '';
        const session = uploadSessions.get(sessionId);
        if (!session) {
            await route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ error: 'Session not found' }) });
            return;
        }

        const payload = route.request().postDataJSON() as { filename: string; path?: string };
        const finalPath = [payload.path ?? '', payload.filename].filter(Boolean).join('/');
        const contentMap = (fileContentsByVaultId[vaultId] ??= {});
        contentMap[finalPath] = contentMap[finalPath] ?? '';
        upsertFileNode(vaultId, finalPath, session.uploadedBytes);
        uploadSessions.delete(sessionId);

        await route.fulfill({
            status: 201,
            contentType: 'application/json',
            body: JSON.stringify({ path: finalPath, filename: payload.filename, size: session.uploadedBytes }),
        });
    });

    await page.route('**/api/plugins', async (route) => {
        if (route.request().method() === 'GET') {
            await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ plugins }) });
            return;
        }
        await route.continue();
    });

    await page.route(/.*\/api\/plugins\/[^/]+\/toggle$/, async (route) => {
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ success: true }) });
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/search\?.*/, async (route) => {
        await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({
                results: searchResults,
                total_count: searchResults.length,
                page: 1,
                page_size: 50,
            }),
        });
    });

    await page.route('**/api/preferences', async (route) => {
        const method = route.request().method();
        if (method === 'GET') {
            await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(prefs) });
            return;
        }
        if (method === 'PUT') {
            Object.assign(prefs, route.request().postDataJSON() as Record<string, unknown>);
            await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(prefs) });
            return;
        }
        await route.continue();
    });

    await page.route('**/api/preferences/reset', async (route) => {
        await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({
                theme: 'dark',
                editor_mode: 'raw',
                font_size: 14,
                window_layout: null,
            }),
        });
    });

    // ── Bookmarks ──────────────────────────────────────────────────────────────
    await page.route(/.*\/api\/vaults\/([^/]+)\/bookmarks$/, async (route) => {
        const vaultId = route.request().url().match(/\/vaults\/([^/]+)\/bookmarks/)![1];
        const method = route.request().method();
        if (method === 'GET') {
            await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(bookmarksByVaultId[vaultId] ?? []) });
            return;
        }
        if (method === 'POST') {
            const payload = route.request().postDataJSON() as { path: string; title: string };
            const created = { id: `bm-${Date.now()}`, path: payload.path, title: payload.title };
            (bookmarksByVaultId[vaultId] ??= []).push(created);
            await route.fulfill({ status: 201, contentType: 'application/json', body: JSON.stringify(created) });
            return;
        }
        await route.fallback();
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/bookmarks\/[^/]+$/, async (route) => {
        const url = route.request().url();
        const m = url.match(/\/vaults\/([^/]+)\/bookmarks\/([^/]+)$/);
        if (!m) { await route.fallback(); return; }
        const [, vaultId, bookmarkId] = m;
        if (route.request().method() === 'DELETE') {
            bookmarksByVaultId[vaultId] = (bookmarksByVaultId[vaultId] ?? []).filter((b) => b.id !== bookmarkId);
            await route.fulfill({ status: 204, body: '' });
            return;
        }
        await route.fallback();
    });

    // ── Tags ───────────────────────────────────────────────────────────────────
    await page.route(/.*\/api\/vaults\/([^/]+)\/tags$/, async (route) => {
        const vaultId = route.request().url().match(/\/vaults\/([^/]+)\/tags/)![1];
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(tagsByVaultId[vaultId] ?? []) });
    });

    // ── Backlinks ──────────────────────────────────────────────────────────────
    await page.route(/.*\/api\/vaults\/[^/]+\/backlinks.*/, async (route) => {
        const url = route.request().url();
        const vaultId = url.match(/\/vaults\/([^/]+)\/backlinks/)![1];
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(backlinksByVaultId[vaultId] ?? []) });
    });

    // ── AI outline / suggestions ───────────────────────────────────────────────
    await page.route(/.*\/api\/vaults\/[^/]+\/ml\/outline$/, async (route) => {
        const payload = route.request().postDataJSON() as { file_path?: string };
        const outlineResult = options.outlineResult ?? {
            file_path: payload.file_path ?? '',
            summary: 'Test outline summary',
            sections: [{ line_number: 1, level: 1, title: 'Section One', summary: 'intro' }],
            generated_at: new Date().toISOString(),
        };
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({
            file_path: outlineResult.file_path ?? payload.file_path ?? '',
            summary: outlineResult.summary,
            sections: outlineResult.sections,
            generated_at: outlineResult.generated_at ?? new Date().toISOString(),
        }) });
    });
    await page.route(/.*\/api\/vaults\/[^/]+\/ml\/suggestions$/, async (route) => {
        const payload = route.request().postDataJSON() as { file_path?: string };
        const suggestionsResult = options.suggestionsResult ?? {
            file_path: payload.file_path ?? '',
            suggestions: [
                {
                    id: 'suggest-move',
                    kind: 'move_to_folder' as const,
                    confidence: 0.87,
                    rationale: 'Move to subfolder',
                    target_folder: 'Projects',
                },
                {
                    id: 'suggest-tag',
                    kind: 'tag' as const,
                    confidence: 0.74,
                    rationale: 'Add tags',
                    tag: 'project',
                },
            ],
            existing_tags: [],
            generated_at: new Date().toISOString(),
        };
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({
            file_path: suggestionsResult.file_path ?? payload.file_path ?? '',
            suggestions: suggestionsResult.suggestions,
            existing_tags: suggestionsResult.existing_tags ?? [],
            generated_at: suggestionsResult.generated_at ?? new Date().toISOString(),
        }) });
    });

    // ── Entity / Schema / Graph routes ────────────────────────────────────────
    await page.route('**/api/plugins/entity-types', async (route) => {
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ entity_types: entityTypes }) });
    });

    await page.route('**/api/plugins/relation-types', async (route) => {
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ relation_types: relationTypes }) });
    });

    await page.route('**/api/plugins/labels', async (route) => {
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ labels: [] }) });
    });

    await page.route(/.*\/api\/vaults\/([^/]+)\/graph$/, async (route) => {
        const vaultId = route.request().url().match(/\/vaults\/([^/]+)\/graph/)![1];
        const data = graphByVaultId[vaultId] ?? { nodes: [], edges: [] };
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(data) });
    });

    await page.route(/.*\/api\/vaults\/([^/]+)\/entity-by-path.*/, async (route) => {
        const url = route.request().url();
        const vaultMatch = url.match(/\/vaults\/([^/]+)\/entity-by-path/);
        if (!vaultMatch) { await route.fallback(); return; }
        const vaultId = vaultMatch[1];
        const pathParam = new URL(url).searchParams.get('path') ?? '';
        const byPath = entityByPathByVaultId[vaultId] ?? {};
        const result = byPath[pathParam] ?? { entity: null, relations: [] };
        if (result.entity === null) {
            await route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ error: 'Not found' }) });
        } else {
            await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(result) });
        }
    });

    await page.route(/.*\/api\/vaults\/[^/]+\/entity-template.*/, async (route) => {
        const url = route.request().url();
        const typeId = new URL(url).searchParams.get('type') ?? '';
        const content = entityTemplatesByTypeId[typeId] ?? `---\nlibrarium_type: ${typeId}\n---\n`;
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ content }) });
    });

    await page.route('**/api/admin/entity-index-stats', async (route) => {
        await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(entityIndexStats) });
    });
}
