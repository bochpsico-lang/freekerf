import { describe, expect, it } from 'vitest';

import { pngSize } from '../dev/fake-document';
import { greyOf } from './raster';

describe('raster previews', () => {
  it('grey levels of RGBA pixels; transparent is nothing burnt', () => {
    const rgba = new Uint8ClampedArray([0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 0, 100, 100, 100, 255]);
    expect([...greyOf(rgba)]).toEqual([0, 255, 255, 100]);
  });

  it('the fake reads the size of a PNG', () => {
    const png =
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=';
    expect(pngSize(png)).toEqual([1, 1]);
    expect(pngSize(btoa('not a png at all, really not'))).toBeNull();
  });
});
