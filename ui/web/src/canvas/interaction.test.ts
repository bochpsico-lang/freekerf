import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Camera, type CameraState } from './camera';
import { CanvasInteraction, LONG_PRESS_MS, type InteractionHost, type PointerInput } from './interaction';

function setup(
  mode: 'design' | 'produce' = 'design',
  objectAt: (x: number, y: number) => string | null = () => null,
) {
  let cam: CameraState = Camera.fitted({ x: 0, y: 0, width: 400, height: 300 }, 848, 648, 24);
  const host = {
    mode: () => mode,
    camera: () => cam,
    setCamera: vi.fn((c: CameraState) => {
      cam = c;
    }),
    hitTest: vi.fn(objectAt),
    select: vi.fn(),
    selectArea: vi.fn(),
    marquee: vi.fn(),
    contextMenu: vi.fn(),
  } satisfies InteractionHost;
  return { host, ix: new CanvasInteraction(host), cam: () => cam };
}

const ptr = (x: number, y: number, extra: Partial<PointerInput> = {}): PointerInput => ({
  id: 1,
  x,
  y,
  button: 0,
  type: 'mouse',
  shift: false,
  ctrl: false,
  alt: false,
  ...extra,
});

describe('CanvasInteraction', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('clicks select what is under the pointer', () => {
    const { host, ix } = setup('design', () => 'obj-1');
    ix.pointerDown(ptr(100, 100));
    ix.pointerMove(ptr(101, 101)); // below the threshold
    ix.pointerUp(ptr(101, 101));
    expect(host.select).toHaveBeenCalledWith('obj-1', false);
    ix.pointerDown(ptr(100, 100, { shift: true }));
    ix.pointerUp(ptr(100, 100));
    expect(host.select).toHaveBeenLastCalledWith('obj-1', true);
  });

  it('drags on empty space draw a window or a crossing marquee in Design', () => {
    const { host, ix, cam } = setup('design');
    ix.pointerDown(ptr(100, 100));
    ix.pointerMove(ptr(300, 200));
    expect(host.marquee).toHaveBeenLastCalledWith({ x0: 100, y0: 100, x1: 300, y1: 200, crossing: false });
    ix.pointerUp(ptr(300, 200));
    expect(host.marquee).toHaveBeenLastCalledWith(null);
    const [rect, crossing] = host.selectArea.mock.calls[0]!;
    const [wx, wy] = Camera.toWorld(cam(), 100, 200);
    expect(rect.x).toBeCloseTo(wx);
    expect(rect.y).toBeCloseTo(wy);
    expect(rect.width).toBeCloseTo(100);
    expect(rect.height).toBeCloseTo(50);
    expect(crossing).toBe(false);
    ix.pointerDown(ptr(300, 100));
    ix.pointerMove(ptr(100, 200));
    ix.pointerUp(ptr(100, 200));
    expect(host.selectArea.mock.calls[1]![1]).toBe(true);
  });

  it('drags pan in Produce, with the middle button and with space', () => {
    for (const [mode, extra, space] of [
      ['produce', {}, false],
      ['design', { button: 1 }, false],
      ['design', {}, true],
    ] as const) {
      const { ix, cam } = setup(mode);
      ix.setSpace(space);
      const before = cam();
      ix.pointerDown(ptr(100, 100, extra));
      ix.pointerMove(ptr(150, 130, extra));
      ix.pointerUp(ptr(150, 130, extra));
      expect(cam().cx).toBeCloseTo(before.cx - 50 / before.scale);
      expect(cam().cy).toBeCloseTo(before.cy + 30 / before.scale);
      expect(ix.busy).toBe(false);
    }
  });

  it('the wheel zooms at the cursor; shift+wheel pans', () => {
    const { ix, cam } = setup();
    const s0 = cam().scale;
    const w = Camera.toWorld(cam(), 200, 300);
    ix.wheel({ x: 200, y: 300, dx: 0, dy: -100, shift: false, ctrl: false });
    expect(cam().scale).toBeCloseTo(s0 * 1.2);
    const s = Camera.toScreen(cam(), w[0], w[1]);
    expect(s[0]).toBeCloseTo(200);
    expect(s[1]).toBeCloseTo(300);
    const cx = cam().cx;
    ix.wheel({ x: 0, y: 0, dx: 0, dy: 40, shift: true, ctrl: false });
    expect(cam().cx).toBeCloseTo(cx + 40 / cam().scale);
    const before = cam();
    ix.wheel({ x: 0, y: 0, dx: 0, dy: 0, shift: false, ctrl: true });
    expect(cam()).toEqual(before);
    ix.wheel({ x: 0, y: 0, dx: 30, dy: 10, shift: false, ctrl: false });
    expect(cam().cx).toBeCloseTo(before.cx + 30 / before.scale);
    expect(cam().scale).toBe(before.scale);
  });

  it('a second finger turns the drag into a pinch', () => {
    const { host, ix, cam } = setup('design');
    const touch = (id: number, x: number, y: number) => ptr(x, y, { id, type: 'touch' });
    ix.pointerDown(touch(1, 100, 100));
    ix.pointerMove(touch(1, 130, 100));
    ix.pointerDown(touch(2, 200, 100));
    const s0 = cam().scale;
    ix.pointerMove(touch(2, 230, 100)); // fingers 100 → 100+... apart
    expect(cam().scale).toBeGreaterThan(s0);
    ix.pointerUp(touch(2, 230, 100));
    ix.pointerUp(touch(1, 130, 100));
    expect(host.select).not.toHaveBeenCalled();
    expect(ix.busy).toBe(false);
  });

  it('a long press opens the context menu, a right click too', () => {
    const { host, ix } = setup('design', () => 'obj-2');
    ix.pointerDown(ptr(50, 60, { type: 'touch' }));
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(host.contextMenu).toHaveBeenCalledWith(50, 60, 'obj-2');
    ix.pointerUp(ptr(50, 60, { type: 'touch' }));
    expect(host.select).not.toHaveBeenCalled();
    ix.pointerDown(ptr(70, 80, { button: 2 }));
    expect(host.contextMenu).toHaveBeenLastCalledWith(70, 80, 'obj-2');
  });

  it('a lost pointer drops the gesture', () => {
    const { host, ix } = setup('design');
    ix.pointerDown(ptr(100, 100));
    ix.pointerMove(ptr(200, 200));
    ix.pointerCancel(ptr(200, 200));
    expect(host.marquee).toHaveBeenLastCalledWith(null);
    expect(ix.busy).toBe(false);
    ix.pointerMove(ptr(220, 220)); // ignored
    expect(host.selectArea).not.toHaveBeenCalled();
  });
});
