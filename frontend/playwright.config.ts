import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig, devices } from '@playwright/test';

const frontendDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(frontendDir, '..');
const serverPort = Number(process.env.PLAYWRIGHT_SERVER_PORT ?? 4173);
const baseURL = process.env.PLAYWRIGHT_BASE_URL ?? `http://127.0.0.1:${serverPort}`;
const serverStateDir = '/tmp/codex-playwright';
const osRelease = (() => {
    try {
        return fs.readFileSync('/etc/os-release', 'utf8');
    } catch {
        return '';
    }
})();
const isFedoraHost = /\nID=fedora\n/.test(`\n${osRelease}\n`);
const includeWebkit = process.env.PLAYWRIGHT_INCLUDE_WEBKIT === '1' || process.env.CI || !isFedoraHost;

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

export default defineConfig({
    testDir: './tests',
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
            `rm -f ${serverStateDir}/codex.db`,
            `printf '%s\n' '[server]' 'host = "127.0.0.1"' 'port = ${serverPort}' '' '[database]' 'path = "${serverStateDir}/codex.db"' '' '[auth]' 'enabled = true' 'bootstrap_admin_username = "admin"' 'bootstrap_admin_password = "admin"' > ${serverStateDir}/config.toml`,
            'npm --prefix frontend run build',
            'cargo build -p codex-server --bin codex',
            `./target/debug/codex --config ${serverStateDir}/config.toml`,
        ].join(' && '),
        url: `${baseURL}/api/health`,
        reuseExistingServer: !process.env.CI,
        timeout: 180 * 1000,
    },
    projects,
});
