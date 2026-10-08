import { describe, expect, it } from 'vitest';

import { benchOptions, DEFAULT_BENCH } from './bench';

describe('benchOptions', () => {
  it('keeps the defaults of PLAN M4a', () => {
    expect(benchOptions(new URLSearchParams(''))).toEqual(DEFAULT_BENCH);
    expect(DEFAULT_BENCH).toMatchObject({ paths: 20_000, imageWidth: 8000, imageHeight: 6000 });
  });

  it('reads diagnostics parameters', () => {
    expect(benchOptions(new URLSearchParams('paths=0&image=16x8&ms=500&zooms=1,2'))).toEqual({
      paths: 0,
      imageWidth: 16,
      imageHeight: 8,
      phaseMs: 500,
      zooms: [1, 2],
    });
    expect(benchOptions(new URLSearchParams('paths=x&image=big&zooms=-1')).paths).toBe(20_000);
  });
});
