// design/ux.md flows 3 and 7 in the real app, on the emulator: import an SVG,
// tune a layer, make the job from the design at the head's position, frame, burn
// it after the fire confirmation; then pause, resume, STOP and recover the job.
import { $, browser, expect } from '@wdio/globals';

import { byTestId, connectEmulator, loadProgram, pickFile, snapshot, waitForText } from './helpers';

const SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" width="12mm" height="8mm" viewBox="0 0 12 8">' +
  '<rect x="1" y="1" width="4" height="4" stroke="#E8622C" fill="none"/>' +
  '<circle cx="9" cy="4" r="2" stroke="#10A37F" fill="none"/></svg>';
const LONG = [
  'G21 G90',
  'M4 S0',
  ...Array.from({ length: 400 }, (_, i) => `G1 X${(i % 2) * 50} Y${i * 0.1} F600 S200`),
  'M5',
  '',
].join('\n');

/** Presses Fire laser once its countdown is over. */
async function fire() {
  const button = await byTestId('fire');
  await expect(button).toBeEnabled({ wait: 5_000 });
  await button.click();
  await expect(byTestId('dialog-fire_confirmation')).not.toBeExisting({ wait: 5_000 });
}

describe('From a drawing to a burned job, and recovering one', () => {
  it('flow 3: import an SVG by colour layer and tune a layer', async () => {
    await byTestId('state-chip').waitForDisplayed({ timeout: 30_000 });
    await byTestId('mode-design').click();
    await $('[data-tool="import"]').click();
    await pickFile('import-file', 'logo.svg', 'image/svg+xml', SVG);
    await byTestId('import-go').click();
    await browser.waitUntil(async () => (await snapshot()).document.objects.length === 2, {
      timeout: 10_000,
    });
    const row = await $(`[data-testid="layer-list"] .layer[style*="#E8622C"]`);
    await row.$('[aria-label="Edit layer settings"]').click();
    const speed = await row.$('[data-testid="layer-speed"]');
    await speed.setValue('6000');
    await browser.keys('Enter');
    await browser.waitUntil(
      async () =>
        (await snapshot()).document.layers.find((l) => l.color === '#E8622C')?.speed_mm_min === 6000,
    );
  });

  it('flow 3: the job from the design at the head, framed, then burned after the confirmation', async () => {
    await connectEmulator();
    const box = await byTestId('design-job');
    await box.$('[data-start="current_position"]').click();
    await box.$('[data-anchor="center"]').click();
    await byTestId('design-job-go').click();
    await waitForText('program-name', 'document');
    await byTestId('frame').click();
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Ready'), { wait: 30_000 });
    await byTestId('start').click();
    await fire();
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Running'), { wait: 10_000 });
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Ready'), { wait: 60_000 });
  });

  it('flow 7: pause and resume, STOP, then recover the job from the banner', async () => {
    await loadProgram('long.nc', LONG);
    await waitForText('program-name', 'long.nc');
    await byTestId('start').click();
    await fire();
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Running'), { wait: 10_000 });
    await byTestId('pause').click();
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Paused'), { wait: 10_000 });
    await byTestId('resume').click();
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Running'), { wait: 10_000 });
    await byTestId('estop').click();
    await byTestId('alarm-banner').waitForDisplayed({ timeout: 10_000 });
    if (await byTestId('banner-unlock').isExisting()) await byTestId('banner-unlock').click();
    await byTestId('banner-recover').click();
    const dialog = await byTestId('dialog-recover');
    await dialog.waitForDisplayed();
    // no homing on the emulator
    const homing = await dialog.$('label=Home ($H) before going on').$('input');
    if ((await homing.isExisting()) && (await homing.isSelected())) await homing.click();
    await byTestId('recover-go').click();
    await fire();
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Running'), { wait: 10_000 });
    await byTestId('estop').click();
    await expect(byTestId('state-chip')).not.toHaveText(expect.stringContaining('Running'), { wait: 10_000 });
  });
});
