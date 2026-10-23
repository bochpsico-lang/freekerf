// PLAN §9.5 / M4a acceptance: no laser action runs without the human confirmation.
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '../App.svelte';
import { FakeService } from '../dev/fake-service';
import { ACTION_TIERS } from '../gen/api';
import { projections } from '../start';
import { designJob } from './design-job.svelte';

const LASER = Object.entries(ACTION_TIERS)
  .filter(([, tier]) => tier === 'laser')
  .map(([name]) => name);

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return f === 'dev' || f === 'gen' ? [] : sources(p);
    return /\.(ts|svelte)$/.test(f) && !f.endsWith('.test.ts') ? [p] : [];
  });
}

describe('the laser gate', () => {
  it('knows the laser actions from the generated catalogue', () => {
    expect(LASER.sort()).toEqual(['job.resume', 'job.run', 'machine.custom_code', 'machine.send']);
  });

  it('no UI code dispatches a laser action directly', () => {
    const offenders: string[] = [];
    for (const file of sources(join(process.cwd(), 'src'))) {
      const text = readFileSync(file, 'utf8');
      for (const action of LASER) {
        const re = new RegExp(`dispatch\\(\\s*['"\`]${action.replace('.', '\\.')}['"\`]`);
        if (re.test(text)) offenders.push(`${file}: ${action}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});

describe('fire confirmation dialog', () => {
  beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }));
  afterEach(() => vi.useRealTimers());

  async function ready() {
    const service = new FakeService();
    await service.dispatch('machine.connect', { emulator: true });
    await service.dispatch('job.load_text', { name: 'coaster.nc', gcode: 'G1 X1' });
    const props = projections(service);
    await Promise.all([props.machine.refresh(), props.view.refresh(), props.settings.refresh()]);
    render(App, props);
    await fireEvent.click(screen.getByTestId('start'));
    await screen.findByTestId('dialog-fire_confirmation');
    return service;
  }

  it('fires only after the countdown, through confirmFire', async () => {
    const service = await ready();
    expect(screen.getByTestId('fire-summary').textContent).toContain('coaster.nc');
    const fire = screen.getByTestId('fire') as HTMLButtonElement;
    expect(fire.disabled).toBe(true);
    expect(fire.textContent).toContain('Fire in 3 s');
    await fireEvent.click(fire);
    expect(service.fired).toEqual([]);
    service.now = 3000;
    await vi.advanceTimersByTimeAsync(3000);
    await waitFor(() => expect(fire.disabled).toBe(false));
    expect(fire.textContent).toContain('Fire laser');
    await fireEvent.click(fire);
    await waitFor(() => expect(service.fired).toEqual([{ action: 'job.run', params: {} }]));
    await waitFor(() => expect(screen.queryByTestId('dialog-fire_confirmation')).toBeNull());
    // the window never dispatched it itself
    expect(service.calls.map((c) => c.action)).not.toContain('job.run');
  });

  it('a refused confirmation shows why', async () => {
    const service = await ready();
    const id = service.view.fire_request!.id;
    await service.confirmFire(id).catch(() => undefined);
    expect(service.fired).toEqual([]);
  });

  it('cancel, Esc and the backdrop: cancel or keep, never fire', async () => {
    const service = await ready();
    await fireEvent.click(screen.getByTestId('dialog-backdrop'));
    expect(service.view.fire_request).not.toBeNull();
    await fireEvent.keyDown(window, { key: 'Escape' });
    await waitFor(() => expect(service.view.fire_request).toBeNull());
    await fireEvent.click(screen.getByTestId('start'));
    await screen.findByTestId('dialog-fire_confirmation');
    await fireEvent.click(screen.getByTestId('fire-cancel'));
    await waitFor(() => expect(service.view.fire_request).toBeNull());
    expect(service.fired).toEqual([]);
  });

  it('is asked in front of the Produce preview', async () => {
    const service = await ready();
    expect(service.view.mode).toBe('produce');
  });

  async function outOfDate(source: 'design' | 'file') {
    const service = new FakeService();
    await service.dispatch('machine.connect', { emulator: true });
    await service.dispatch('document.add_shape', { shape: 'rect', x: 0, y: 0, width: 20, height: 10 });
    if (source === 'design') await service.dispatch('job.from_document', {});
    else await service.dispatch('job.load_text', { name: 'coaster.nc', gcode: 'G1 X1' });
    await service.dispatch('document.add_shape', { shape: 'rect', x: 30, y: 0, width: 5, height: 5 });
    const props = projections(service);
    await Promise.all([props.machine.refresh(), props.view.refresh(), props.settings.refresh()]);
    render(App, props);
    await props.doc.refresh();
    await service.dispatch('view.set_mode', { mode: 'produce' });
    await waitFor(() => expect(screen.getByTestId('job-stale')).toBeTruthy());
    await fireEvent.click(screen.getByTestId('start'));
    await screen.findByTestId('dialog-fire_confirmation');
    return service;
  }

  it('an out-of-date job fires only once the person accepts it as it is', async () => {
    const service = await outOfDate('design');
    expect(screen.getByTestId('fire-stale').dataset.source).toBe('design');
    service.now = 3000;
    await vi.advanceTimersByTimeAsync(3000);
    const fire = screen.getByTestId('fire') as HTMLButtonElement;
    expect(fire.disabled).toBe(true);
    // the app refuses it without the acceptance too
    await expect(service.confirmFire(service.view.fire_request!.id)).rejects.toThrow();
    await fireEvent.click(screen.getByTestId('fire-accept-stale'));
    await waitFor(() => expect(fire.disabled).toBe(false));
    await fireEvent.click(fire);
    await waitFor(() => expect(service.fired).toEqual([{ action: 'job.run', params: {} }]));
  });

  it('an out-of-date G-code file can be replaced by the job from the design', async () => {
    const service = await outOfDate('file');
    const stale = screen.getByTestId('fire-stale');
    expect(stale.dataset.source).toBe('file');
    expect(stale.textContent).toContain('coaster.nc');
    await fireEvent.click(screen.getByTestId('fire-update'));
    await waitFor(() => expect(service.view.fire_request).toBeNull());
    await waitFor(() => expect(service.state.program?.name).toBe('document'));
    expect(service.lastJobPlacement).toEqual({ start: designJob.start, anchor: designJob.anchor });
    expect(service.fired).toEqual([]);
  });

  it('STOP stays clickable over the confirmation', async () => {
    const service = await ready();
    await fireEvent.click(screen.getByTestId('estop'));
    expect(service.calls.map((c) => c.action)).toContain('machine.stop');
  });
});
