// tests/e2e — real-server integration suite (drives the actual API, no
// mocks). See playwright.shared.ts for why this has its own server/port/
// state dir, separate from playwright.config.ts's ui/ suite.
import { createPlaywrightConfig } from './playwright.shared';

export default createPlaywrightConfig({
    testDir: './tests/e2e',
    defaultPort: 4174,
    stateDir: '/tmp/librarium-playwright-e2e',
});
