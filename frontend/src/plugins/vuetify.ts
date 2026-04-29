// Vuetify plugin — dark Obsidian-inspired theme
import 'vuetify/styles';
import '@mdi/font/css/materialdesignicons.css';
import { createVuetify } from 'vuetify';
import { aliases, mdi } from 'vuetify/iconsets/mdi';

// Dark Obsidian theme
const obsidianDark = {
    dark: true,
    colors: {
        background: '#111111',  // --bg-primary
        surface: '#0a0a0a',     // --bg-secondary (sidebar/card bg)
        'surface-bright': '#2a2a2a',  // --bg-tertiary (hover states)
        'surface-light': '#1a1a1a',   // panels
        'on-background': '#e5e7eb',   // --text-primary
        'on-surface': '#e5e7eb',
        primary: '#5b83f5',     // --accent-color (periwinkle blue)
        'primary-darken-1': '#272cd9',
        secondary: '#9ca3af',   // --text-secondary
        'secondary-darken-1': '#6b7280',
        error: '#ef4444',
        info: '#3b82f6',
        success: '#22c55e',
        warning: '#f59e0b',
        border: '#27272a',      // --border-color (Zinc 800)
    },
};

// Light Obsidian theme
const obsidianLight = {
    dark: false,
    colors: {
        background: '#ffffff',
        surface: '#f8f9fa',
        'surface-bright': '#f0f2f5',
        'surface-light': '#f8f9fa',
        'on-background': '#111827',
        'on-surface': '#111827',
        primary: '#2363de',     // strong blue
        'primary-darken-1': '#272cd9',
        secondary: '#4b5563',
        'secondary-darken-1': '#374151',
        error: '#ef4444',
        info: '#3b82f6',
        success: '#22c55e',
        warning: '#f59e0b',
        border: '#e5e7eb',
    },
};

export const vuetify = createVuetify({
    icons: {
        defaultSet: 'mdi',
        aliases,
        sets: { mdi },
    },
    theme: {
        defaultTheme: 'obsidianDark',
        themes: { obsidianDark, obsidianLight },
    },
    defaults: {
        VBtn: { variant: 'text', density: 'comfortable' },
        VTextField: { variant: 'outlined', density: 'compact', hideDetails: 'auto' },
        VSelect: { variant: 'outlined', density: 'compact', hideDetails: 'auto' },
    },
});
