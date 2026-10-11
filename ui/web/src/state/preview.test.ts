import { describe, expect, it } from 'vitest';

import { FakeService } from '../dev/fake-service';
import { PreviewState } from './preview.svelte';

describe('PreviewState', () => {
  it('loads the tool path when a program is loaded and follows the head', async () => {
    const service = new FakeService();
    const p = new PreviewState(service);
    expect(p.toolpath).toBeNull();
    await service.dispatch('job.load_text', { name: 'a.nc', gcode: 'G1 X1\nG1 X2\n' });
    await p.reload();
    expect(p.toolpath?.segmentCount).toBe(4);
    service.emitHead(10, 20, 3, 8, true);
    expect(p.head).toMatchObject({ x: 10, y: 20, executed: 3, target: 8, running: true });
  });

  it('reports a malformed tool path', async () => {
    const service = new FakeService();
    service.toolpath = async () => new ArrayBuffer(10);
    const p = new PreviewState(service);
    await p.reload();
    expect(p.error).toContain('too short');
    expect(p.toolpath).toBeNull();
  });
});
