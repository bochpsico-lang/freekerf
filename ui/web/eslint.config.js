// ESLint: TypeScript + Svelte, and the architecture rules of the UI (AGENTS.md,
// "UI rules"): only `src/ipc/` talks to Tauri; nothing outside `src/dev/` and the
// tests imports the fake service.
import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

import freekerf from './eslint-rules/index.js';
import svelteConfig from './svelte.config.js';

const tauriOutsideIpc = {
  group: ['@tauri-apps/*'],
  message: 'Only src/ipc/ talks to Tauri; use the AppService passed to the component.',
};
const devOutsideDev = {
  group: ['**/dev/*', '**/dev', './dev/*', '../dev/*'],
  message: 'The fake service (src/dev/) is for development and tests only; it must not reach production.',
};

export default ts.config(
  { ignores: ['dist/', 'coverage/', 'node_modules/', 'src/gen/'] },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs.recommended,
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: {
      parserOptions: {
        projectService: true,
        extraFileExtensions: ['.svelte'],
        parser: ts.parser,
        svelteConfig,
      },
    },
  },
  {
    files: ['src/**/*.{ts,svelte}'],
    ignores: ['src/ipc/**', 'src/dev/**', 'src/**/*.test.ts'],
    rules: { 'no-restricted-imports': ['error', { patterns: [tauriOutsideIpc, devOutsideDev] }] },
  },
  {
    // dispatch-only: app state is read-only outside src/state (and the fake service)
    files: ['src/**/*.{ts,svelte}'],
    ignores: ['src/state/**', 'src/dev/**', 'src/**/*.test.ts'],
    plugins: { freekerf },
    rules: {
      'freekerf/no-app-state-mutation': 'error',
      'no-restricted-globals': [
        'error',
        { name: 'localStorage', message: 'App state lives in Rust: use settings or view actions.' },
        { name: 'sessionStorage', message: 'App state lives in Rust: use settings or view actions.' },
        { name: 'indexedDB', message: 'App state lives in Rust: use settings or view actions.' },
      ],
    },
  },
  {
    files: ['src/ipc/**/*.ts'],
    ignores: ['src/**/*.test.ts'],
    rules: { 'no-restricted-imports': ['error', { patterns: [devOutsideDev] }] },
  },
  {
    files: ['src/dev/**'],
    rules: { 'no-restricted-imports': ['error', { patterns: [tauriOutsideIpc] }] },
  },
  {
    rules: {
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
    },
  },
);
