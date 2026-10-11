import { describe, expect, it, vi } from 'vitest';

import { Camera, type CameraState } from '../canvas/camera';
import { FakeService } from '../dev/fake-service';
import type { Tool } from '../gen/api';
import { documentSummary, type DocumentSummary } from '../state/document.svelte';
import { DEFAULT_SIZE, DesignEditor } from './editor';

const MOUSE = { shift: false, alt: false, touch: false };

async function setup(tool: Tool = 'select') {
  const service = new FakeService();
  const read = async () => documentSummary(await service.dispatch('session.snapshot', {}));
  let summary: DocumentSummary = await read();
  const refresh = async () => {
    summary = await read();
  };
  const cam: CameraState = Camera.fitted({ x: 0, y: 0, width: 200, height: 100 }, 800, 400, 0);
  const host = {
    doc: () => summary,
    tool: () => tool,
    camera: () => cam,
    coarse: () => false,
    service,
    run: async (f: () => Promise<unknown>) => {
      await f();
      await refresh();
    },
    selectTool: vi.fn(),
    textPlaced: vi.fn(),
    changed: vi.fn(),
  };
  const editor = new DesignEditor(host);
  const add = async (x: number, y: number, w = 20, h = 10) => {
    const { id } = await service.dispatch('document.add_shape', { shape: 'rect', x, y, width: w, height: h });
    await refresh();
    return id;
  };
  return {
    service,
    read,
    editor,
    host,
    cam,
    add,
    refresh,
    summary: () => summary,
    setTool: (t: Tool) => (tool = t),
  };
}

describe('DesignEditor', () => {
  it('hit tests the topmost object on a visible layer, with a few pixels of tolerance', async () => {
    const { editor, add, service, refresh, read } = await setup();
    const a = await add(10, 10);
    const b = await add(20, 12);
    expect(editor.hitTest(25, 15)).toBe(String(b));
    expect(editor.hitTest(12, 11)).toBe(String(a));
    expect(editor.hitTest(9.5, 11)).toBe(String(a)); // 0.5 mm = 2 px at 4 px/mm
    expect(editor.hitTest(100, 90)).toBeNull();
    const s = await read();
    await service.dispatch('document.update_layer', { id: s.layers[0]!.id, visible: false });
    await refresh();
    expect(editor.hitTest(25, 15)).toBeNull();
  });

  it('click, shift-click and marquee selection', async () => {
    const { editor, add, summary, service } = await setup();
    const a = await add(10, 10);
    const b = await add(50, 10);
    editor.select(String(a), false);
    await vi.waitFor(() => expect(summary().selection).toEqual([a]));
    editor.select(String(b), true);
    await vi.waitFor(() => expect(summary().selection).toEqual([a, b]));
    editor.select(String(a), false, true); // a drag starts on a selected object: kept
    editor.select(null, false);
    await vi.waitFor(() => expect(summary().selection).toEqual([]));
    // window: only what is inside; crossing: what it touches
    editor.selectArea({ x: 0, y: 0, width: 40, height: 40 }, false, false);
    await vi.waitFor(() => expect(summary().selection).toEqual([a]));
    editor.selectArea({ x: 25, y: 0, width: 30, height: 40 }, true, false);
    await vi.waitFor(() => expect(summary().selection).toEqual([a, b]));
    const selects = service.calls.filter((c) => c.action === 'document.select');
    expect(selects.at(-1)!.params).toEqual({ ids: [a, b], mode: 'replace' });
  });

  it('drags a handle with a preview, then sends one transform on release', async () => {
    const { editor, add, summary, service, host, cam } = await setup();
    const a = await add(10, 10, 20, 10);
    // the se handle of the selection box (30, 10) on screen
    const [sx, sy] = Camera.toScreen(cam, 30, 10);
    expect(editor.hitHandle(sx, sy)).toBe('se');
    expect(editor.hitHandle(...Camera.toScreen(cam, 20, 15))).toBe('move');
    editor.drag('start', 'se', [30, 10], MOUSE);
    editor.drag('move', 'se', [50, 10], MOUSE);
    expect(host.changed).toHaveBeenCalled();
    const ghost = editor.overlay()!.ghost!;
    expect(ghost.map(([x]) => Math.round(x))).toContain(50);
    expect(summary().objects[0]!.bounds).toEqual([10, 10, 30, 20]); // nothing sent yet
    editor.drag('end', 'se', [50, 10], MOUSE);
    await vi.waitFor(() => expect(summary().objects[0]!.bounds![2]).toBeCloseTo(50));
    // corners keep the proportions: the height doubled too
    expect(summary().objects[0]!.bounds![1]).toBeCloseTo(0);
    expect(service.calls.filter((c) => c.action === 'document.transform')).toHaveLength(1);
    expect(editor.overlay()!.ghost).toBeNull();
    void a;
  });

  it('a cancelled drag sends nothing; an empty move sends nothing', async () => {
    const { editor, add, service } = await setup();
    await add(10, 10);
    editor.drag('start', 'move', [15, 15], MOUSE);
    editor.drag('move', 'move', [40, 40], MOUSE);
    expect(editor.busy).toBe(true);
    editor.drag('cancel', 'move', [40, 40], MOUSE);
    expect(editor.busy).toBe(false);
    editor.drag('start', 'move', [15, 15], MOUSE);
    editor.drag('end', 'move', [15, 15], MOUSE);
    expect(service.calls.filter((c) => c.action === 'document.transform')).toHaveLength(0);
  });

  it('rotation snaps to 15° with Shift', async () => {
    const { editor, add, summary } = await setup();
    await add(0, 0, 20, 20);
    editor.drag('start', 'rotate', [10, 30], MOUSE);
    editor.drag('end', 'rotate', [0.5, 19], { ...MOUSE, shift: true });
    await vi.waitFor(() => {
      const t = summary().objects[0]!.transform;
      expect((Math.atan2(t[1], t[0]) * 180) / Math.PI).toBeCloseTo(45);
    });
  });

  it('drawing tools place shapes with a click or a box, then go back to select', async () => {
    const { editor, summary, host, setTool } = await setup('rect');
    expect(editor.placing()).toBe(true);
    editor.place({ x: 100, y: 50, width: 0, height: 0 });
    await vi.waitFor(() => expect(summary().objects).toHaveLength(1));
    const [w, h] = DEFAULT_SIZE.rect;
    expect(summary().objects[0]!.bounds).toEqual([100 - w / 2, 50 - h / 2, 100 + w / 2, 50 + h / 2]);
    expect(host.selectTool).toHaveBeenCalled();
    setTool('ellipse');
    editor.place({ x: 0, y: 0, width: 30, height: 20 });
    await vi.waitFor(() => expect(summary().objects).toHaveLength(2));
    expect(summary().objects[1]!.bounds![2]).toBeCloseTo(30);
    setTool('text');
    editor.place({ x: 5, y: 5, width: 0, height: 0 });
    await vi.waitFor(() => expect(host.textPlaced).toHaveBeenCalled());
    expect(summary().objects[2]!.kind).toBe('text');
    setTool('select');
    expect(editor.placing()).toBe(false);
  });

  it('nudges the selection by 1, 10 or 0.1 mm', async () => {
    const { editor, add, summary } = await setup();
    await add(10, 10);
    await editor.nudge(1, 0, { shift: false, alt: false });
    await editor.nudge(0, -1, { shift: true, alt: false });
    await editor.nudge(1, 0, { shift: false, alt: true });
    const b = summary().objects[0]!.bounds!;
    expect(b[0]).toBeCloseTo(11.1);
    expect(b[1]).toBeCloseTo(0);
    expect(editor.toWorld(0, 0)).not.toBeNull();
  });
});
