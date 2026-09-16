// tests/ui — mocked-API UI suite. See playwright.shared.ts for why this has
// its own server/port/state dir, separate from playwright.e2e.config.ts.
import { createPlaywrightConfig } from './playwright.shared';

export default createPlaywrightConfig({
    testDir: './tests/ui',
    defaultPort: 4173,
    stateDir: '/tmp/librarium-playwright-ui',
});
