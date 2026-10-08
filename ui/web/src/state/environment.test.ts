import { describe, expect, it } from 'vitest';

import { Environment, layoutFor, resolveDensity } from './environment.svelte';

describe('environment', () => {
  it('layouts by width (design/ux.md)', () => {
    expect(layoutFor(1180)).toBe('wide');
    expect(layoutFor(1179)).toBe('medium');
    expect(layoutFor(820)).toBe('medium');
    expect(layoutFor(819)).toBe('narrow');
    expect(layoutFor(375)).toBe('narrow');
  });

  it('density: setting first, else the last input', () => {
    expect(resolveDensity('compact', 'touch')).toBe('compact');
    expect(resolveDensity('comfortable', 'mouse')).toBe('comfortable');
    expect(resolveDensity('auto', 'touch')).toBe('comfortable');
    expect(resolveDensity('auto', 'pen')).toBe('compact');
  });

  it('follows the window and the last pointer', () => {
    const env = new Environment(window);
    window.innerWidth = 900;
    window.dispatchEvent(new Event('resize'));
    expect(env.layout).toBe('medium');
    const down = new Event('pointerdown') as PointerEvent;
    Object.defineProperty(down, 'pointerType', { value: 'touch' });
    window.dispatchEvent(down);
    expect(env.input).toBe('touch');
    env.dispose();
    window.innerWidth = 1300;
    window.dispatchEvent(new Event('resize'));
    expect(env.layout).toBe('medium');
    expect(new Environment(null).layout).toBe('wide');
  });
});
