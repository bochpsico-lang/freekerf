// design/ux.md flows 1 and 2 in the real app: the first run opens the welcome by
// itself; importing from LaserGRBL (its files only read) writes FreeKerf's
// settings, laser counters and button scripts; exploring connects the simulator.
import { $, browser, expect } from '@wdio/globals';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

import { byTestId } from './helpers';

const home = () => process.env.FREEKERF_HOME!;
const settingsFile = () => readFileSync(join(home(), 'settings.toml'), 'utf8');

describe('First run and the LaserGRBL migration', () => {
  it('flow 1: the welcome opens by itself on the first run', async () => {
    await byTestId('state-chip').waitForDisplayed({ timeout: 30_000 });
    await expect(byTestId('dialog-welcome')).toBeDisplayed({ wait: 10_000 });
  });

  it('flow 2: imports the chosen items from LaserGRBL and reports them', async () => {
    await byTestId('welcome-migrate').click();
    const items = await byTestId('migrate-items');
    await expect(items).toHaveText(expect.stringContaining('5 custom buttons'), { wait: 10_000 });
    await expect(items).toHaveText(expect.stringContaining('2 laser modules'));
    await byTestId('migrate-go').click();
    const report = await byTestId('migrate-report');
    await expect(report).toHaveText(expect.stringContaining('5 custom buttons'), { wait: 10_000 });
    // written: the connection (LaserGRBL's 250000 baud), the button scripts
    expect(settingsFile()).toContain('baud = 250000');
    expect(existsSync(join(home(), 'macros', 'buttons.json'))).toBe(true);
    expect(existsSync(join(home(), 'machine-cache.json'))).toBe(true);
    // closing records that the welcome was seen
    await byTestId('welcome-finish').click();
    await expect(byTestId('dialog-welcome')).not.toBeExisting();
    await browser.waitUntil(() => settingsFile().includes('welcomed = true'), { timeout: 5_000 });
  });

  it('flow 1: explore with the simulator connects the emulator', async () => {
    await byTestId('open-settings').click();
    await $('button=Show the welcome again').click();
    await byTestId('dialog-welcome').waitForDisplayed();
    await byTestId('welcome-explore').click();
    await expect(byTestId('dialog-welcome')).not.toBeExisting();
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Ready'), { wait: 15_000 });
  });
});
