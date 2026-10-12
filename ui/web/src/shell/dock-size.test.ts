import { describe, expect, it } from 'vitest';

import {
  DOCK_MIN_PX,
  LAYERS_MAX_PCT,
  LAYERS_MIN_PCT,
  clampLayers,
  clampWidth,
  keyResize,
  layersAt,
  widthAt,
} from './dock-size';

describe('dock sizes', () => {
  it('keeps the width between the minimum and half the window', () => {
    expect(clampWidth(100, 1600)).toBe(DOCK_MIN_PX);
    expect(clampWidth(400.4, 1600)).toBe(400);
    expect(clampWidth(1200, 1600)).toBe(800);
    // a window narrower than twice the minimum still gets the minimum
    expect(clampWidth(400, 300)).toBe(DOCK_MIN_PX);
  });

  it('keeps Layers between the limits', () => {
    expect(clampLayers(0)).toBe(LAYERS_MIN_PCT);
    expect(clampLayers(50.6)).toBe(51);
    expect(clampLayers(100)).toBe(LAYERS_MAX_PCT);
  });

  it('follows the pointer', () => {
    // dock on the right edge of a 1600 px window: dragging its left edge left widens it
    expect(widthAt(1200, 1600, 1600)).toBe(400);
    expect(widthAt(1500, 1600, 1600)).toBe(DOCK_MIN_PX);
    expect(widthAt(0, 1600, 1600)).toBe(800);
    // a dock from y=100 to y=500
    expect(layersAt(300, 100, 500)).toBe(50);
    expect(layersAt(90, 100, 500)).toBe(LAYERS_MIN_PCT);
    expect(layersAt(600, 100, 500)).toBe(LAYERS_MAX_PCT);
    expect(layersAt(300, 100, 100)).toBe(LAYERS_MIN_PCT);
  });

  it('resizes with the keyboard', () => {
    expect(keyResize('width', 'ArrowLeft', false, 400, 1600)).toBe(416);
    expect(keyResize('width', 'ArrowRight', true, 400, 1600)).toBe(336);
    expect(keyResize('width', 'Home', false, 400, 1600)).toBe(DOCK_MIN_PX);
    expect(keyResize('width', 'End', false, 400, 1600)).toBe(800);
    expect(keyResize('width', 'ArrowUp', false, 400, 1600)).toBeNull();
    expect(keyResize('layers', 'ArrowDown', false, 46, 1600)).toBe(48);
    expect(keyResize('layers', 'ArrowUp', true, 46, 1600)).toBe(38);
    expect(keyResize('layers', 'Home', false, 46, 1600)).toBe(LAYERS_MIN_PCT);
    expect(keyResize('layers', 'End', false, 46, 1600)).toBe(LAYERS_MAX_PCT);
    expect(keyResize('layers', 'ArrowLeft', false, 46, 1600)).toBeNull();
  });
});
