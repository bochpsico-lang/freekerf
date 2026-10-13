// design/ux.md flows 5 and 6 in the real app, by the screens only: a text and a
// rectangle, subtract, a 3 × 2 array, the cut layer; then the material test.
import { $, browser, expect } from '@wdio/globals';

import { byTestId, setNumber, snapshot } from './helpers';

const count = async () => (await snapshot()).document.objects.length;

describe('Design flows from scratch', () => {
  it('flow 5: text and a rectangle, subtract, array, cut', async () => {
    await byTestId('state-chip').waitForDisplayed({ timeout: 30_000 });
    await byTestId('mode-design').click();
    const canvas = await byTestId('canvas');
    // R and a click: a 40 × 30 mm plate; T and a click: "Text" inside it
    await $('[data-tool="rect"]').click();
    await canvas.click();
    await browser.waitUntil(async () => (await count()) === 1, { timeout: 10_000 });
    await $('[data-tool="text"]').click();
    await canvas.click();
    await browser.waitUntil(async () => (await count()) === 2, { timeout: 10_000 });
    // the text is typed on the canvas right away: Enter keeps it (and gives the keys back)
    const editor = await byTestId('text-editor');
    await editor.waitForDisplayed();
    await browser.keys('Enter');
    await editor.waitForDisplayed({ reverse: true });
    // both selected (Ctrl+A), then Subtract in the inspector: one plate with a hole
    await browser.keys(['Control', 'a']);
    await browser.waitUntil(async () => (await snapshot()).selection.length === 2);
    await byTestId('obj-difference').click();
    await browser.waitUntil(async () => (await count()) === 1, { timeout: 10_000 });
    // a 3 × 2 array
    await setNumber('obj-array-columns', 3);
    await setNumber('obj-array-rows', 2);
    await byTestId('obj-array-go').click();
    await browser.waitUntil(async () => (await count()) === 6, { timeout: 10_000 });
    // the palette: a new colour layer for the six, then its mode to cut
    await $('.palette [data-color="#E8622C"]').click();
    await browser.waitUntil(async () => {
      const s = await snapshot();
      const layer = s.document.layers.find((l) => l.color === '#E8622C');
      return !!layer && s.document.objects.every((o) => o.layer === layer.id);
    });
    const row = await $(`[data-testid="layer-list"] .layer[style*="#E8622C"]`);
    await row.$('[aria-label="Edit layer settings"]').click();
    await row.$('button=Cut').click();
    await browser.waitUntil(
      async () => (await snapshot()).document.layers.find((l) => l.color === '#E8622C')?.mode === 'cut',
    );
  });

  it('flow 6: the material test from the tool rail', async () => {
    const before = (await snapshot()).document.layers.length;
    await $('[data-tool="test"]').click();
    await byTestId('dialog-material-test').waitForDisplayed();
    await setNumber('mtest-power-steps', 3);
    await setNumber('mtest-speed-steps', 2);
    await expect(byTestId('mtest-cells')).toHaveText(expect.stringContaining('6 cells'));
    await byTestId('mtest-go').click();
    await expect(byTestId('dialog-material-test')).not.toBeExisting();
    await browser.waitUntil(async () => (await snapshot()).document.layers.length === before + 7, {
      timeout: 10_000,
    });
    const cells = (await snapshot()).document.layers.filter((l) => l.mode === 'fill');
    expect(cells.map((l) => l.speed_mm_min).sort((a, b) => a - b)).toEqual([500, 500, 500, 6000, 6000, 6000]);
  });
});
