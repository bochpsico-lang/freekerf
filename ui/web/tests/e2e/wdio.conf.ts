// End-to-end tests of the desktop app (the real binary, the real service, the
// built-in Grbl emulator: never a serial device) with WebdriverIO and
// tauri-driver (Linux: WebKitWebDriver). Run with `npm run test:e2e` after
// `npm run build` and `cargo build -p freekerf-desktop --features custom-protocol`
// (see doc/features/desktop-app.md).
import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../../../..');
const app = process.env.FREEKERF_DESKTOP ?? join(root, 'target/debug/freekerf-desktop');
let driver: ChildProcess | null = null;

/** A fresh app home per session: default settings, a 1 s fire countdown. */
function freshHome(): string {
  const home = mkdtempSync(join(tmpdir(), 'freekerf-e2e-'));
  writeFileSync(
    join(home, 'settings.toml'),
    [
      'version = 1',
      '',
      '[ui]',
      'language = "en"',
      'welcomed = true',
      '',
      '[safety]',
      'fire_countdown_s = 1',
      '',
    ].join('\n'),
  );
  return home;
}

export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./specs/**/*.e2e.ts'],
  maxInstances: 1,
  hostname: '127.0.0.1',
  port: 4444,
  capabilities: [{ 'tauri:options': { application: app } } as WebdriverIO.Capabilities],
  logLevel: 'warn',
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: { ui: 'bdd', timeout: 60_000 },
  waitforTimeout: 10_000,

  // one app (and one home) per spec file
  beforeSession() {
    process.env.FREEKERF_HOME = freshHome();
    driver = spawn(process.env.TAURI_DRIVER ?? 'tauri-driver', [], {
      stdio: [null, process.stdout, process.stderr],
      env: process.env,
    });
  },
  afterSession() {
    driver?.kill();
    driver = null;
  },
};
