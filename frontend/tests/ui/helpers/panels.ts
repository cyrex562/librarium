import { expect, type Page } from '@playwright/test';

/**
 * Expand a collapsible sidebar panel by its header selector (e.g.
 * '.bookmarks-header'). Sidebar panels start collapsed, and the header is a
 * toggle — clicking unconditionally would collapse an already-open panel —
 * so this only clicks when the chevron shows it's closed.
 */
export async function expandPanel(page: Page, headerSelector: string) {
    const header = page.locator(headerSelector).first();
    await expect(header).toBeVisible();
    if (await header.locator('.mdi-chevron-right').isVisible().catch(() => false)) {
        await header.click();
    }
    await expect(header.locator('.mdi-chevron-down')).toBeVisible();
}

/**
 * Expand the editor's Frontmatter panel (a Vuetify v-expansion-panel, which
 * starts collapsed). Its title renders as a button carrying aria-expanded.
 */
export async function expandFrontmatter(page: Page) {
    const title = page.getByRole('button', { name: 'Frontmatter' });
    await expect(title).toBeVisible();
    if ((await title.getAttribute('aria-expanded')) !== 'true') {
        await title.click();
    }
    await expect(title).toHaveAttribute('aria-expanded', 'true');
}
