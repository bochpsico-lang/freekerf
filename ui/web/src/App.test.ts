import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import App from './App.svelte';
import { FakeService } from './dev/fake-service';
import { MachineState } from './state/machine.svelte';
import { PreviewState } from './state/preview.svelte';

describe('App', () => {
  it('connects to the emulator and keeps STOP visible', async () => {
    const service = new FakeService();
    const machine = new MachineState(service);
    await machine.refresh();
    render(App, { service, machine, preview: new PreviewState(service) });
    expect(screen.getByRole('status').textContent).toBe('Disconnected');
    await fireEvent.click(screen.getByText('Connect to the emulator'));
    await machine.refresh();
    expect(await screen.findByText('Ready')).toBeTruthy();
    await fireEvent.click(screen.getByText('STOP'));
    expect(service.calls.map((c) => c.action)).toContain('machine.stop');
  });

  it('shows action errors', async () => {
    const service = new FakeService();
    service.on('machine.connect', () => {
      throw new Error('no emulator');
    });
    const machine = new MachineState(service);
    render(App, { service, machine, preview: new PreviewState(service) });
    await fireEvent.click(screen.getByText('Connect to the emulator'));
    expect(await screen.findByText('no emulator')).toBeTruthy();
  });
});
