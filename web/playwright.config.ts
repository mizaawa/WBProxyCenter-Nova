import { defineConfig } from '@playwright/test'
export default defineConfig({
  testDir: 'tests',
  projects: [
    { name: 'first-visit', testMatch: '**/first-visit.setup.ts' },
    { name: 'console', testMatch: '**/*.spec.ts', dependencies: ['first-visit'] },
  ],
  workers: 1,
  timeout: 30000,
  reporter: 'list',
  use: {
    baseURL: process.env.WBP_E2E_URL || 'http://127.0.0.1:3040',
    browserName: 'chromium',
    viewport: { width: 1280, height: 800 },
    trace: 'off',
  },
})
