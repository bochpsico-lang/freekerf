import { describe, expect, it } from 'vitest';

import { formatDuration, formatLength } from './format';

describe('format', () => {
  it('lengths', () => {
    expect(formatLength(12.345, 'mm')).toBe('12.35');
    expect(formatLength(25.4, 'in')).toBe('1.000');
  });
  it('durations', () => {
    expect(formatDuration(0)).toBe('0:00');
    expect(formatDuration(65_000)).toBe('1:05');
    expect(formatDuration(3_725_000)).toBe('1:02:05');
    expect(formatDuration(-5)).toBe('0:00');
  });
});
