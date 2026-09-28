import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { vuetify } from '@/plugins/vuetify';

// #146: Typst Preview, PDF export and conversion need the server's compiler.
// In local (thin-client / Android) mode they must be absent, not broken.
let localMode = false;
vi.mock('@/api/client', () => ({
    isLocalTransportActive: vi.fn(() => localMode),
    ApiError: class ApiError extends Error {},
    apiRenderTypst: vi.fn(),
    apiExportPdf: vi.fn(),
    apiConvertToTypst: vi.fn(),
    apiConvertToMarkdown: vi.fn(),
    apiResolveWikiLink: vi.fn(),
}));

import TypstEditor from './TypstEditor.vue';

function mountEditor() {
    return mount(TypstEditor, {
        props: { tabId: 't1', vaultId: 'v1', content: '= Hello', filePath: 'a.typ', mode: 'formatted_raw' },
        global: { plugins: [vuetify] },
    });
}

describe('TypstEditor capabilities', () => {
    beforeEach(() => {
        setActivePinia(createPinia());
    });

    it('offers Preview, PDF export and Convert to Markdown against a server', () => {
        localMode = false;
        const w = mountEditor();
        expect(w.find('[data-testid="typst-export-pdf"]').exists()).toBe(true);
        expect(w.find('[data-testid="typst-convert-markdown"]').exists()).toBe(true);
        expect(w.findAll('button').some((b) => b.text() === 'Preview')).toBe(true);
    });

    it('hides them in local mode, keeping Plain and Formatted editing', () => {
        localMode = true;
        const w = mountEditor();
        expect(w.find('[data-testid="typst-export-pdf"]').exists()).toBe(false);
        expect(w.find('[data-testid="typst-convert-markdown"]').exists()).toBe(false);
        const labels = w.findAll('button').map((b) => b.text());
        expect(labels).not.toContain('Preview');
        expect(labels).toContain('Plain');
        expect(labels).toContain('Formatted');
    });

    it('treats a stored Preview mode as Formatted in local mode', () => {
        localMode = true;
        const w = mount(TypstEditor, {
            props: { tabId: 't1', vaultId: 'v1', content: '= Hello', filePath: 'a.typ', mode: 'fully_rendered' },
            global: { plugins: [vuetify] },
        });
        expect(w.find('[data-testid="typst-preview"]').exists()).toBe(false);
        expect(w.find('.typst-editor').isVisible()).toBe(true);
    });
});
