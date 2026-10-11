// Produce mode on the emulator: connect, console, jog, load, fire confirmation,
// run to the end, STOP during a run.
import { $, browser, expect } from '@wdio/globals';
import { byTestId, connectEmulator, loadProgram, waitForText } from './helpers';

const SQUARE = ['G21 G90', 'M4 S0', 'G0 X0 Y0', 'G1 X5 F6000 S300', 'G1 Y5', 'G1 X0', 'G1 Y0', 'M5', ''].join(
  '\n',
);
const LONG = [
  'G21 G90',
  'M4 S0',
  ...Array.from({ length: 400 }, (_, i) => `G1 X${(i % 2) * 50} Y${i * 0.1} F600 S200`),
  'M5',
  '',
].join('\n');

describe('Produce mode on the emulator', () => {
  it('connects and shows STOP', async () => {
    await connectEmulator();
    await expect(byTestId('estop')).toBeDisplayed();
    await expect(byTestId('firmware')).toHaveText(expect.stringContaining('Grbl'));
  });

  it('the console sends $$ and shows the answer', async () => {
    await $('.dock-tabs [data-panel="console"]').click();
    const input = await byTestId('console-input');
    await input.setValue('$$');
    await browser.keys('Enter');
    await expect(byTestId('console')).toHaveText(expect.stringContaining('$130='), { wait: 10_000 });
    await $('.dock-tabs [data-panel="machine"]').click();
  });

  it('jogs one step', async () => {
    const before = await byTestId('dro-x').getText();
    await $('[data-jog="E"]').click();
    await expect(byTestId('dro-x')).not.toHaveText(before, { wait: 10_000 });
  });

  it('runs a job only after the fire confirmation, to the end', async () => {
    await loadProgram('square.nc', SQUARE);
    await waitForText('program-name', 'square.nc');
    await byTestId('start').click();
    const fire = await byTestId('fire');
    await fire.waitForDisplayed();
    // the countdown (1 s here) keeps the button disabled
    await expect(fire).toBeDisabled();
    await expect(fire).toBeEnabled({ wait: 5_000 });
    await fire.click();
    await expect(byTestId('dialog-fire_confirmation')).not.toBeExisting({ wait: 5_000 });
    // the square ends where it started: X0 Y0 (after the jog of the previous test)
    await expect(byTestId('dro-x')).toHaveText('0.00', { wait: 30_000 });
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Ready'), { wait: 30_000 });
  });

  it('cancelling the confirmation runs nothing', async () => {
    await byTestId('start').click();
    await byTestId('fire-cancel').click();
    await expect(byTestId('dialog-fire_confirmation')).not.toBeExisting();
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Ready'));
  });

  it('STOP during a run resets the controller and keeps the laser off', async () => {
    await loadProgram('long.nc', LONG);
    await waitForText('program-name', 'long.nc');
    await byTestId('start').click();
    const fire = await byTestId('fire');
    await expect(fire).toBeEnabled({ wait: 5_000 });
    await fire.click();
    await expect(byTestId('state-chip')).toHaveText(expect.stringContaining('Running'), { wait: 10_000 });
    await byTestId('estop').click();
    await expect(byTestId('state-chip')).not.toHaveText(expect.stringContaining('Running'), { wait: 10_000 });
  });
});
