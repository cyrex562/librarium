/**
 * Backlinks Plugin
 * Shows all notes that link to the current note
 */

class BacklinksPlugin {
    constructor(api) {
        this.api = api;
        this.config = {};
        this.currentFile = null;
        this.backlinksCache = new Map();
    }

    async onLoad(ctx) {
        console.log('Backlinks plugin loaded', ctx);

        // Load configuration
        const savedConfig = await this.api.storage_get('config');
        this.config = savedConfig || this.getDefaultConfig();

        // Inject a sidebar panel element that displayBacklinks() will update.
        // The panel is appended to the plugin sidebar container when the host
        // exposes one; if no container exists yet the panel is held in memory
        // and attached on first use.
        this.panelEl = document.createElement('div');
        this.panelEl.id   = 'librarium-backlinks-plugin-panel';
        this.panelEl.className = 'plugin-panel backlinks-plugin-panel';

        const container = document.getElementById('plugin-sidebar-panels');
        if (container) {
            container.appendChild(this.panelEl);
        }

        // Build initial backlinks index
        await this.rebuildIndex();

        console.log('Backlinks plugin initialized');
    }

    async onFileOpen(ctx, filePath) {
        this.currentFile = filePath;
        await this.showBacklinks(filePath);
    }

    async onFileSave(ctx, filePath) {
        // Rebuild index when files are saved
        await this.updateBacklinksForFile(filePath);
    }

    async onUnload() {
        if (this.panelEl && this.panelEl.parentNode) {
            this.panelEl.parentNode.removeChild(this.panelEl);
        }
        this.panelEl = null;
        console.log('Backlinks plugin unloaded');
    }

    getDefaultConfig() {
        return {
            show_unlinked_mentions: true,
            case_sensitive: false
        };
    }

    async rebuildIndex() {
        console.log('Building backlinks index...');
        this.backlinksCache.clear();

        try {
            const files = await this.api.list_files(this.api.getContext().vault_id, '*.md');

            for (const file of files) {
                await this.updateBacklinksForFile(file);
            }

            console.log(`Backlinks index built: ${this.backlinksCache.size} files indexed`);
        } catch (error) {
            console.error('Failed to build backlinks index:', error);
        }
    }

    async updateBacklinksForFile(filePath) {
        try {
            const content = await this.api.read_file(this.api.getContext().vault_id, filePath);
            const links = this.extractLinks(content);

            // Store outgoing links for this file
            this.backlinksCache.set(filePath, links);
        } catch (error) {
            console.error(`Failed to update backlinks for ${filePath}:`, error);
        }
    }

    extractLinks(content) {
        const links = [];

        // Extract wiki links [[Note Name]]
        const wikiLinkRegex = /\[\[([^\]]+)\]\]/g;
        let match;

        while ((match = wikiLinkRegex.exec(content)) !== null) {
            let linkText = match[1];

            // Handle [[Note|Alias]] format
            if (linkText.includes('|')) {
                linkText = linkText.split('|')[0];
            }

            links.push(linkText.trim());
        }

        return links;
    }

    async showBacklinks(filePath) {
        const backlinks = this.findBacklinks(filePath);
        const unlinkedMentions = this.config.show_unlinked_mentions
            ? await this.findUnlinkedMentions(filePath)
            : [];

        // Display backlinks in UI
        this.displayBacklinks(backlinks, unlinkedMentions);
    }

    findBacklinks(targetFile) {
        const backlinks = [];
        const targetName = this.getFileNameWithoutExtension(targetFile);

        for (const [sourceFile, links] of this.backlinksCache.entries()) {
            if (sourceFile === targetFile) continue;

            for (const link of links) {
                const linkName = this.getFileNameWithoutExtension(link);
                if (linkName === targetName) {
                    backlinks.push({
                        file: sourceFile,
                        type: 'link'
                    });
                    break;
                }
            }
        }

        return backlinks;
    }

    async findUnlinkedMentions(targetFile) {
        const mentions = [];
        const targetName = this.getFileNameWithoutExtension(targetFile);

        try {
            const files = await this.api.list_files(this.api.getContext().vault_id, '*.md');

            for (const file of files) {
                if (file === targetFile) continue;

                try {
                    const content = await this.api.read_file(this.api.getContext().vault_id, file);

                    // Remove wiki links to avoid double counting
                    const contentWithoutLinks = content.replace(/\[\[[^\]]+\]\]/g, '');

                    // Search for mentions
                    const searchText = this.config.case_sensitive
                        ? contentWithoutLinks
                        : contentWithoutLinks.toLowerCase();
                    const searchTerm = this.config.case_sensitive
                        ? targetName
                        : targetName.toLowerCase();

                    if (searchText.includes(searchTerm)) {
                        mentions.push({
                            file: file,
                            type: 'mention'
                        });
                    }
                } catch (error) {
                    // Skip files that can't be read
                }
            }
        } catch (error) {
            console.error('Failed to find unlinked mentions:', error);
        }

        return mentions;
    }

    displayBacklinks(backlinks, unlinkedMentions) {
        const all = [
            ...backlinks.map(b => ({ ...b, label: 'link' })),
            ...unlinkedMentions.map(m => ({ ...m, label: 'mention' })),
        ];

        // Render into the injected panel element when available.
        if (this.panelEl) {
            // Lazily attach to DOM if the container appeared after onLoad.
            if (!this.panelEl.parentNode) {
                const container = document.getElementById('plugin-sidebar-panels');
                if (container) container.appendChild(this.panelEl);
            }

            if (all.length === 0) {
                this.panelEl.innerHTML = '<p class="backlinks-empty">No backlinks found.</p>';
                return;
            }

            const items = all.map(entry => {
                const fileName = entry.file.split('/').pop() || entry.file;
                const badge = entry.label === 'mention'
                    ? '<span class="backlink-badge mention">mention</span>'
                    : '';
                return `<div class="backlink-entry" data-path="${this.escapeAttr(entry.file)}" role="button" tabindex="0">
                    <span class="backlink-name">${this.escapeHtml(fileName)}</span>${badge}
                    <span class="backlink-path">${this.escapeHtml(entry.file)}</span>
                </div>`;
            }).join('');

            this.panelEl.innerHTML = `
                <div class="backlinks-panel-header">
                    Backlinks
                    <span class="backlinks-count">(${all.length})</span>
                </div>
                <div class="backlinks-entries">${items}</div>
            `;

            // Wire click/keyboard navigation — open the file via the API if available.
            this.panelEl.querySelectorAll('.backlink-entry').forEach(el => {
                const open = () => {
                    const path = el.dataset.path;
                    if (path && this.api.open_file) {
                        this.api.open_file(this.api.getContext().vault_id, path);
                    }
                };
                el.addEventListener('click', open);
                el.addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' ') open(); });
            });
        }
    }

    escapeHtml(str) {
        return String(str)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;');
    }

    escapeAttr(str) {
        return String(str).replace(/"/g, '&quot;').replace(/'/g, '&#39;');
    }

    getFileNameWithoutExtension(filePath) {
        const fileName = filePath.split('/').pop() || filePath;
        return fileName.replace(/\.md$/, '');
    }
}

export default BacklinksPlugin;
