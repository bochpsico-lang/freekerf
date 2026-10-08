// Safety and the shell in the real app: the window cannot run a laser action
// without the confirmation, Shift+Esc is STOP from anywhere, settings reach disk.
import { $, browser, expect } from '@wdio/globals';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { byTestId, connectEmulator } from './helpers';

/** Calls the app's `dispatch` command directly, as a compromised page would. */
const rawDispatch = (action: string, params: unknown) =>
  browser.execute(
    async (a: string, p: unknown) => {
      const w = window as unknown as {
        __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
      };
      try {
        await w.__TAURI_INTERNALS__.invoke('dispatch', { action: a, params: p });
        return 'ok';
      } catch (e) {
        return `error: ${JSON.stringify(e)}`;
      }
    },
    action,
    params,
  );

describe('Safety in the real app', () => {
  it('a laser action from the window is refused without the confirmation', async () => {
    await connectEmulator();
    const r = await rawDispatch('machine.send', { line: 'M3 S1000' });
    expect(r).toContain('error');
    expect(r).toContain('permission');
    // asking is allowed, and opens the confirmation
    expect(await rawDispatch('fire.request', { action: 'machine.send', params: { line: 'M3 S1000' } })).toBe(
      'ok',
    );
    await expect(byTestId('dialog-fire_confirmation')).toBeDisplayed();
    await byTestId('fire-cancel').click();
    await expect(byTestId('dialog-fire_confirmation')).not.toBeExisting();
  });

  it('Shift+Esc is STOP, even from a text field', async () => {
    await $('.dock-tabs [data-panel="console"]').click();
    const consoleText = async () => String(await (await byTestId('console')).getProperty('textContent'));
    await byTestId('console-input').setValue('$I');
    await browser.keys('Enter');
    await browser.waitUntil(async () => (await consoleText()).includes('[VER:'));
    await byTestId('console-input').click();
    await browser.action('key').down('\uE008').down('\uE00C').up('\uE00C').up('\uE008').perform();
    // the soft reset: the controller starts again and the console shows its banner only
    await browser.waitUntil(async () => !(await consoleText()).includes('[VER:'), { timeout: 10_000 });
    expect(await consoleText()).toContain('Grbl');
    await $('.dock-tabs [data-panel="machine"]').click();
  });

  it('settings changed in the window are saved to settings.toml', async () => {
    await byTestId('open-settings').click();
    await byTestId('theme-light').click();
    await expect($('html')).toHaveAttribute('data-theme', 'light');
    await browser.waitUntil(() =>
      readFileSync(join(process.env.FREEKERF_HOME!, 'settings.toml'), 'utf8').includes('theme = "light"'),
    );
    await browser.keys('Escape');
    await expect(byTestId('dialog-settings')).not.toBeExisting();
  });
});
