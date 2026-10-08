import { describe, expect, it } from 'vitest';
import { flushSync } from 'svelte';

import { FakeService } from '../dev/fake-service';
import { MachineState, visualState } from './machine.svelte';

describe('visualState', () => {
  it('groups the firmware states as in design/ux.md', () => {
    expect(visualState(undefined)).toBe('offline');
    expect(visualState('Disconnected')).toBe('offline');
    expect(visualState('Connecting')).toBe('connecting');
    for (const s of ['Idle', 'Check', 'Queue'] as const) expect(visualState(s)).toBe('idle');
    for (const s of ['Run', 'Jog', 'Home', 'Cooling', 'Tool'] as const) expect(visualState(s)).toBe('run');
    for (const s of ['Hold', 'Door', 'AutoHold'] as const) expect(visualState(s)).toBe('hold');
    expect(visualState('Alarm')).toBe('alarm');
  });
});

describe('MachineState', () => {
  it('follows state.get and the events', async () => {
    const service = new FakeService();
    const m = new MachineState(service);
    await m.refresh();
    flushSync();
    expect(m.visual).toBe('offline');
    await service.dispatch('machine.connect', { emulator: true });
    await m.refresh();
    flushSync();
    expect(m.visual).toBe('idle');
    expect(service.calls.map((c) => c.action)).toContain('state.get');
  });

  it('coalesces concurrent refreshes', async () => {
    const service = new FakeService();
    const m = new MachineState(service);
    await Promise.all([m.refresh(), m.refresh(), m.refresh()]);
    expect(service.calls.filter((c) => c.action === 'state.get')).toHaveLength(1);
  });
});
