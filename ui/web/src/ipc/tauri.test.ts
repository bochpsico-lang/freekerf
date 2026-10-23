import { beforeEach, describe, expect, it, vi } from 'vitest';

// a plain function (not vi.fn) so rejected promises are not tracked as unhandled
const ipc = vi.hoisted(() => ({
  calls: [] as unknown[][],
  impl: (..._args: unknown[]): Promise<unknown> => Promise.resolve(undefined),
}));
const invoke = {
  mockReset: () => {
    ipc.calls = [];
    ipc.impl = () => Promise.resolve(undefined);
  },
  mockResolvedValue: (v: unknown) => {
    ipc.impl = () => Promise.resolve(v);
  },
  mockImplementation: (f: () => Promise<unknown>) => {
    ipc.impl = f;
  },
  get mock() {
    return { calls: ipc.calls };
  },
};
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => {
    ipc.calls.push(args);
    return ipc.impl(...args);
  },
  Channel: class {
    onmessage: (m: unknown) => void = () => {};
  },
}));

import { ActionFailure } from '../service';
import { TauriService } from './tauri';

describe('TauriService', () => {
  beforeEach(() => invoke.mockReset());

  it('dispatches through the dispatch command', async () => {
    invoke.mockResolvedValue({ name: 'a.nc' });
    const s = new TauriService();
    await expect(s.dispatch('job.info', {})).resolves.toEqual({ name: 'a.nc' });
    expect(ipc.calls).toEqual([['dispatch', { action: 'job.info', params: {} }]]);
  });

  it('runs the slow actions through the background command', async () => {
    invoke.mockResolvedValue({ png: '', width: 1, height: 1 });
    await new TauriService().dispatch('document.raster_preview', { id: 3 });
    expect(ipc.calls).toEqual([
      ['dispatch_background', { action: 'document.raster_preview', params: { id: 3 } }],
    ]);
  });

  it('turns action errors into ActionFailure', async () => {
    invoke.mockImplementation(() =>
      Promise.reject({ kind: 'permission_denied', message: { action: 'job.run', needed: 'laser' } }),
    );
    const s = new TauriService();
    const e = await s.dispatch('job.run', {}).catch((x: unknown) => x);
    expect(e).toBeInstanceOf(ActionFailure);
    expect((e as ActionFailure).message).toBe('job.run: permission denied (needs tier laser)');
  });

  it('rethrows other errors', async () => {
    invoke.mockImplementation(() => Promise.reject(new Error('ipc down')));
    await expect(new TauriService().dispatch('state.get', {})).rejects.toThrow('ipc down');
  });

  it('opens one channel and fans events out', () => {
    invoke.mockResolvedValue(undefined);
    const s = new TauriService();
    const a = vi.fn();
    const b = vi.fn();
    s.subscribe(a);
    const stopB = s.subscribe(b);
    expect(ipc.calls).toHaveLength(1);
    const channel = (invoke.mock.calls[0]?.[1] as { events: { onmessage: (m: unknown) => void } }).events;
    channel.onmessage({ event: 'program_loaded' });
    stopB();
    channel.onmessage({ event: 'settings_changed' });
    expect(a).toHaveBeenCalledTimes(2);
    expect(b).toHaveBeenCalledTimes(1);
  });

  it('reads the catalogue', async () => {
    invoke.mockResolvedValue([]);
    await expect(new TauriService().catalog()).resolves.toEqual([]);
    expect(ipc.calls).toEqual([['catalog']]);
  });
});
