// Shared Playwright config factory for the two isolated test suites:
// tests/ui (mocked API, tests/ui/helpers/appMocks.ts) and tests/e2e (drives
// the real server end to end, tests/e2e/pages/*).
//
// Each suite gets its OWN server process, port, and SQLite state directory —
// see playwright.config.ts and playwright.e2e.config.ts. They used to share
// one server, and the real-server e2e/ specs left state (real users, real
// vaults, failed-login counters) that the mocked ui/ specs then fell through
// to for any endpoint their fixture hadn't mocked, via the shared fixture's
// fake bearer token getting a real 401 from a real server. Measured cost:
// running both against one server took the ui/ suite from 173 passed / 4
// failed down to 144 / 47. Fully separate server instances make that
// contamination structurally impossible, regardless of which endpoints get
// mocked in the future.
import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig, devices, type PlaywrightTestConfig } from '@playwright/test';

export interface SuiteOptions {
    /** Directory (relative to frontend/) holding this suite's *.spec.ts files. */
    testDir: string;
    /** Default port for this suite's dedicated server. Override with PLAYWRIGHT_SERVER_PORT. */
    defaultPort: number;
    /** Dedicated state directory (config.toml + librarium.db) for this suite's server. */
    stateDir: string;
}

export function createPlaywrightConfig(opts: SuiteOptions): PlaywrightTestConfig {
    const frontendDir = path.dirname(fileURLToPath(import.meta.url));
    const repoRoot = path.resolve(frontendDir, '..');
    const serverPort = Number(process.env.PLAYWRIGHT_SERVER_PORT ?? opts.defaultPort);
    const baseURL = process.env.PLAYWRIGHT_BASE_URL ?? `http://127.0.0.1:${serverPort}`;
    const serverStateDir = opts.stateDir;
    const osRelease = (() => {
        try {
            return fs.readFileSync('/etc/os-release', 'utf8');
        } catch {
            return '';
        }
    })();
    const isFedoraHost = /\nID=fedora\n/.test(`\n${osRelease}\n`);
    const includeWebkit = process.env.PLAYWRIGHT_INCLUDE_WEBKIT === '1' || !!process.env.CI || !isFedoraHost;

    const projects = [
        {
            name: 'chromium',
            use: { ...devices['Desktop Chrome'] },
        },
        {
            name: 'firefox',
            use: { ...devices['Desktop Firefox'] },
        },
    ];

    if (includeWebkit) {
        projects.push({
            // webkit approximates WebKitGTK 2.36 behaviour for logic testing.
            // Full WebKitGTK 2.36 gate runs in the webkit-compat CI job
            // (ubuntu-22.04 with WebKitGTK installed).
            name: 'webkit',
            use: { ...devices['Desktop Safari'] },
        });
    } else {
        console.warn('Skipping Playwright WebKit project on local Fedora host; set PLAYWRIGHT_INCLUDE_WEBKIT=1 to force it.');
    }

    return defineConfig({
        testDir: opts.testDir,
        testMatch: '**/*.spec.ts',
        fullyParallel: false, // Run sequentially for better stability
        forbidOnly: !!process.env.CI,
        retries: process.env.CI ? 2 : 1,
        workers: 1, // Single worker to avoid conflicts
        reporter: 'list',
        use: {
            baseURL,
            trace: 'on-first-retry',
            screenshot: 'only-on-failure',
        },
        webServer: {
            cwd: repoRoot,
            command: [
                `mkdir -p ${serverStateDir}`,
                `rm -f ${serverStateDir}/librarium.db`,
                `printf '%s\n' '[server]' 'host = "127.0.0.1"' 'port = ${serverPort}' '' '[database]' 'path = "${serverStateDir}/librarium.db"' '' '[auth]' 'enabled = true' 'bootstrap_admin_username = "admin"' 'bootstrap_admin_password = "admin"' > ${serverStateDir}/config.toml`,
                'npm --prefix frontend run build',
                'cargo build -p librarium-server --bin librarium',
                `./target/debug/librarium --config ${serverStateDir}/config.toml`,
            ].join(' && '),
            url: `${baseURL}/api/health`,
            reuseExistingServer: !process.env.CI,
            timeout: 180 * 1000,
        },
        projects,
    });
}
