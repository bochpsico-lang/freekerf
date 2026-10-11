import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import App from '../App.svelte';
import { FakeService } from '../dev/fake-service';
import { projections } from '../start';

async function mountConnected() {
  const service = new FakeService();
  await service.dispatch('machine.connect', { emulator: true });
  const props = projections(service);
  await Promise.all([props.machine.refresh(), props.view.refresh(), props.settings.refresh()]);
  render(App, props);
  service.calls.length = 0;
  return { service, props };
}

const actions = (s: FakeService) => s.calls.map((c) => c.action);

describe('Produce mode', () => {
  it('opens a G-code file and loads it as text', async () => {
    const { service, props } = await mountConnected();
    const input = screen.getByTestId('gcode-file') as HTMLInputElement;
    const file = new File(['G0 X10\nG1 X20 S100\n'], 'part.nc', { type: 'text/plain' });
    Object.defineProperty(input, 'files', { value: [file] });
    await fireEvent.change(input);
    await waitFor(() =>
      expect(service.calls.find((c) => c.action === 'job.load_text')?.params).toEqual({
        name: 'part.nc',
        gcode: 'G0 X10\nG1 X20 S100\n',
      }),
    );
    await props.machine.refresh();
    await waitFor(() => expect(screen.getByTestId('program-name').textContent).toContain('part.nc'));
  });

  it('makes the job from the design, placed by the job start and the anchor', async () => {
    const { service, props } = await mountConnected();
    await service.dispatch('view.set_mode', { mode: 'produce' });
    expect(screen.queryByTestId('design-job')).toBeNull();
    await service.dispatch('document.add_shape', { shape: 'rect', x: 0, y: 0, width: 20, height: 10 });
    await props.doc.refresh();
    const box = await screen.findByTestId('design-job');
    const center = box.querySelector<HTMLButtonElement>('[data-anchor="center"]')!;
    expect(center.disabled).toBe(true); // absolute: no anchor
    await fireEvent.click(box.querySelector('[data-start="user_origin"]')!);
    await waitFor(() => expect(center.disabled).toBe(false));
    await fireEvent.click(center);
    await fireEvent.click(screen.getByTestId('design-job-go'));
    await waitFor(() => expect(service.lastJobPlacement).toEqual({ start: 'user_origin', anchor: 'center' }));
    await props.machine.refresh();
    await waitFor(() => expect(screen.getByTestId('program-name').textContent).toContain('document'));
    expect(screen.getByTestId('job-estimate').textContent).not.toBe('—');
  });

  it('jogs one step per press with the selected step', async () => {
    const { service } = await mountConnected();
    const n = screen.getByTestId('jogpad').querySelector('[data-jog="N"]')!;
    await fireEvent.pointerDown(n);
    expect(service.calls.at(-1)).toEqual({ action: 'machine.jog', params: { direction: 'N', step_mm: 1 } });
  });

  it('continuous jog is a dead-man control', async () => {
    const { service, props } = await mountConnected();
    service.settings.machine = { continuous_jog: true } as never;
    await props.settings.refresh();
    const e = screen.getByTestId('jogpad').querySelector('[data-jog="E"]')!;
    await fireEvent.pointerDown(e);
    expect(service.calls.at(-1)).toEqual({ action: 'machine.jog', params: { direction: 'E' } });
    await fireEvent.pointerUp(e);
    expect(actions(service).at(-1)).toBe('machine.jog_stop');
    // losing the window focus also stops it
    await fireEvent.pointerDown(e);
    window.dispatchEvent(new Event('blur'));
    expect(actions(service).at(-1)).toBe('machine.jog_stop');
    // and leaving the button
    await fireEvent.pointerDown(e);
    await fireEvent.pointerLeave(e);
    expect(actions(service).filter((a) => a === 'machine.jog_stop')).toHaveLength(3);
  });

  it('home, unlock, origin, frame and overrides are machine actions', async () => {
    const { service, props } = await mountConnected();
    await service.dispatch('job.load_text', { name: 'a.nc', gcode: 'G1 X1' });
    await props.machine.refresh();
    for (const id of ['home', 'unlock', 'set-origin', 'frame']) await fireEvent.click(screen.getByTestId(id));
    expect(actions(service)).toEqual(
      expect.arrayContaining(['machine.home', 'machine.unlock', 'machine.set_zero', 'job.frame']),
    );
    await fireEvent.change(screen.getByLabelText('Power'), { target: { value: '60' } });
    expect(service.calls.at(-1)).toEqual({
      action: 'machine.overrides',
      params: { feed: 100, rapid: 100, power: 60 },
    });
  });

  it('Start never dispatches a laser action: it asks for the confirmation', async () => {
    const { service, props } = await mountConnected();
    await service.dispatch('job.load_text', { name: 'a.nc', gcode: 'G1 X1' });
    await props.machine.refresh();
    await fireEvent.click(screen.getByTestId('start'));
    await screen.findByTestId('dialog-fire_confirmation');
    expect(actions(service)).not.toContain('job.run');
  });

  it('console: $ commands go to the machine, G-code asks for the confirmation', async () => {
    const { service } = await mountConnected();
    await service.dispatch('view.open_panel', { name: 'console' });
    const input = screen.getByTestId('console-input') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: '$$' } });
    await fireEvent.submit(input.form!);
    expect(service.calls.find((c) => c.action === 'machine.system_command')?.params).toEqual({ line: '$$' });
    await fireEvent.input(input, { target: { value: 'M3 S1000' } });
    await fireEvent.submit(input.form!);
    await screen.findByTestId('dialog-fire_confirmation');
    expect(actions(service)).not.toContain('machine.send');
  });

  it('alarm banner and recovery dialog', async () => {
    const { service, props } = await mountConnected();
    service.state.machine.status = 'Alarm';
    service.state.resume = {
      executed: 100,
      sent: 120,
      target: 400,
      issue: 'MachineAlarm',
      homing: true,
      set_wco: false,
    };
    await props.machine.refresh();
    await screen.findByTestId('alarm-banner');
    await fireEvent.click(screen.getByTestId('banner-unlock'));
    expect(actions(service)).toContain('machine.unlock');
    await fireEvent.click(screen.getByTestId('banner-recover'));
    await screen.findByTestId('dialog-recover');
    await fireEvent.click(screen.getByTestId('recover-go'));
    await screen.findByTestId('dialog-fire_confirmation');
    expect(actions(service)).not.toContain('job.resume');
  });
});
