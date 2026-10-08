import { describe, expect, it } from 'vitest';

import { rulerLabel } from './overlay';
import { parseColor } from './theme';

describe('overlay helpers', () => {
  it('formats ruler labels', () => {
    expect(rulerLabel(120, 'mm')).toBe('120');
    expect(rulerLabel(0.5, 'mm')).toBe('0.5');
    expect(rulerLabel(25.4, 'in')).toBe('1');
    expect(rulerLabel(10, 'in')).toBe('0.394');
  });

  it('parses token colours', () => {
    expect(parseColor('#FF0000')).toEqual([1, 0, 0, 1]);
    expect(parseColor('#0f0', 0.5)).toEqual([0, 1, 0, 0.5]);
    expect(parseColor('#00000080')[3]).toBeCloseTo(0.502, 2);
    expect(parseColor('rgb(1,2,3)')).toEqual([0.5, 0.5, 0.5, 1]);
  });
});
