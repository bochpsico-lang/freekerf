import { describe, expect, it } from 'vitest';

import { Camera } from '../canvas/camera';
import { applyOp, dragOp, handleAt, rotationStep, selectionBox } from './transform';

const box = { x: 10, y: 10, width: 20, height: 10 };
const none = { shift: false, alt: false, touch: false };

describe('handles', () => {
  it('move translates by the drag', () => {
    expect(dragOp('move', box, [12, 12], [17, 9], none)).toEqual({ translate: [5, -3] });
  });

  it('corners keep proportions unless Shift, from the opposite corner unless Alt', () => {
    const op = dragOp('ne', box, [30, 20], [50, 25], none);
    expect(op).toEqual({ scale: [2, 2], pivot: [10, 10] });
    expect(dragOp('ne', box, [30, 20], [50, 25], { ...none, shift: true })).toEqual({
      scale: [2, 1.5],
      pivot: [10, 10],
    });
    expect(dragOp('ne', box, [30, 20], [40, 25], { ...none, alt: true, shift: true })).toEqual({
      scale: [2, 2],
      pivot: [20, 15],
    });
  });

  it('sides scale one axis', () => {
    expect(dragOp('w', box, [10, 15], [0, 15], none)).toEqual({ scale: [1.5, 1], pivot: [30, 15] });
    expect(dragOp('s', box, [20, 10], [20, 5], none)).toEqual({ scale: [1, 1.5], pivot: [20, 20] });
  });

  it('rotation around the centre, snapped with Shift and on touch', () => {
    const op = dragOp('rotate', box, [20, 25], [10, 15], none);
    expect('rotate_deg' in op && Math.round(op.rotate_deg)).toBe(90);
    expect(rotationStep({ ...none, shift: true })).toBe(15);
    expect(rotationStep({ ...none, touch: true })).toBe(5);
    const snapped = dragOp('rotate', box, [20, 25], [19, 25.3], { ...none, shift: true });
    expect(snapped).toMatchObject({ rotate_deg: 0 });
  });

  it('applies operations to points', () => {
    expect(applyOp({ translate: [1, 2] }, [3, 4])).toEqual([4, 6]);
    expect(applyOp({ scale: [2, 3], pivot: [1, 1] }, [2, 2])).toEqual([3, 4]);
    const [x, y] = applyOp({ rotate_deg: 90, pivot: [0, 0] }, [1, 0]);
    expect(x).toBeCloseTo(0);
    expect(y).toBeCloseTo(1);
  });

  it('finds handles on screen', () => {
    const cam = Camera.fitted({ x: 0, y: 0, width: 100, height: 100 }, 424, 424, 12); // 4 px/mm
    const [sx, sy] = Camera.toScreen(cam, 30, 20);
    expect(handleAt(cam, box, sx, sy, 16, 26)).toBe('ne');
    const [cx, cy] = Camera.toScreen(cam, 20, 15);
    expect(handleAt(cam, box, cx, cy, 16, 26)).toBe('move');
    const [tx, ty] = Camera.toScreen(cam, 20, 20);
    expect(handleAt(cam, box, tx, ty - 26, 16, 26)).toBe('rotate');
    expect(handleAt(cam, box, 0, 0, 16, 26)).toBeNull();
  });

  it('boxes the selection', () => {
    expect(selectionBox([[0, 0, 1, 1], null, [2, -1, 3, 0]])).toEqual({ x: 0, y: -1, width: 3, height: 2 });
    expect(selectionBox([null])).toBeNull();
  });
});
