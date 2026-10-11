// Playwright: the web front end in Chromium against the fake service (dev.html).
// The desktop app itself is driven by WebdriverIO + tauri-driver (tests/e2e).
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: 'tests/web',
  fullyParallel: true,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: 'http://localhost:5174',
    ...devices['Desktop Chrome'],
    launchOptions: process.env.PW_CHROMIUM ? { executablePath: process.env.PW_CHROMIUM } : {},
  },
  webServer: {
    command: 'npx vite --port 5174 --strictPort',
    url: 'http://localhost:5174/dev.html',
    reuseExistingServer: !process.env.CI,
  },
});
