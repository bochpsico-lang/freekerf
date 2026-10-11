import { $, browser, expect } from '@wdio/globals';

/** An element by `data-testid`. */
export const byTestId = (id: string) => $(`[data-testid="${id}"]`);

/** Waits for the shell, goes to the Produce mode and connects to the built-in emulator. */
export async function connectEmulator(): Promise<void> {
  await byTestId('state-chip').waitForDisplayed({ timeout: 30_000 });
  await byTestId('mode-produce').click();
  await expect($('html')).toHaveAttribute('data-mode', 'produce');
  await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Disconnected'));
  await byTestId('connect').click();
  await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Ready'), { wait: 15_000 });
}

/**
 * Loads a G-code program through the panel's file input, as a person picking the
 * file would (the native file picker cannot be driven; WebKitWebDriver does not
 * type into file inputs): the file is set with a DataTransfer and `change` fires.
 */
export async function loadProgram(name: string, gcode: string): Promise<void> {
  const input = await byTestId('gcode-file');
  await browser.execute(
    (el, n, text) => {
      const i = el as unknown as HTMLInputElement;
      const dt = new DataTransfer();
      dt.items.add(new File([text], n, { type: 'text/plain' }));
      i.files = dt.files;
      i.dispatchEvent(new Event('change', { bubbles: true }));
    },
    input,
    name,
    gcode,
  );
}

/**
 * Waits until an element's text contains `text`. Reads `textContent`: WebDriver's
 * visible text is empty for an element scrolled out of its panel.
 */
export async function waitForText(id: string, text: string, timeout = 10_000): Promise<void> {
  const el = await byTestId(id);
  await browser.waitUntil(async () => String(await el.getProperty('textContent')).includes(text), {
    timeout,
    timeoutMsg: `${id} never contained "${text}" (it has "${String(await el.getProperty('textContent'))}")`,
  });
}

/** Runs an action with the window's own `dispatch` command (reads, selection). */
export function dispatch<T = unknown>(action: string, params: unknown = {}): Promise<T> {
  return browser.execute(
    async (a: string, p: unknown) => {
      const w = window as unknown as {
        __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
      };
      return w.__TAURI_INTERNALS__.invoke('dispatch', { action: a, params: p });
    },
    action,
    params,
  ) as Promise<T>;
}

/** The parts of `session.snapshot` the tests read. */
export interface Snapshot {
  selection: number[];
  undo: string[];
  document: {
    layers: { id: number; color: string; mode: string; speed_mm_min: number; power: number }[];
    objects: { id: number; layer: number; kind: string; image?: { tool: string } }[];
  };
}

/** The app's session snapshot. */
export const snapshot = () => dispatch<Snapshot>('session.snapshot');

/** Picks a file in a file input, as a person would (see `loadProgram`). */
export async function pickFile(testid: string, name: string, type: string, text: string): Promise<void> {
  const input = await byTestId(testid);
  await input.waitForExist();
  await browser.execute(
    (el, n, ty, body) => {
      const i = el as unknown as HTMLInputElement;
      const dt = new DataTransfer();
      dt.items.add(new File([body], n, { type: ty }));
      i.files = dt.files;
      i.dispatchEvent(new Event('change', { bubbles: true }));
    },
    input,
    name,
    type,
    text,
  );
}

/** Sets a number field and commits it (Enter). */
export async function setNumber(testid: string, value: number): Promise<void> {
  const el = await byTestId(testid);
  await el.setValue(String(value));
  await browser.keys('Enter');
}
