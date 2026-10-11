import { describe, expect, it } from 'vitest';

import { FakeService } from '../dev/fake-service';
import { base64, importFile, importFormat } from './import';

describe('import', () => {
  it('takes the format from the file name', () => {
    expect(importFormat('Logo.SVG')).toBe('svg');
    expect(importFormat('part.dxf')).toBe('dxf');
    expect(['a.png', 'a.jpg', 'a.jpeg', 'a.bmp', 'a.gif'].map(importFormat)).toEqual(Array(5).fill('image'));
    expect(importFormat('job.nc')).toBeNull();
    expect(importFormat('noext')).toBeNull();
  });

  it('encodes large files in chunks', () => {
    const bytes = new Uint8Array(100_000).map((_, i) => i % 256);
    const decoded = Uint8Array.from(atob(base64(bytes.buffer)), (c) => c.charCodeAt(0));
    expect(decoded).toEqual(bytes);
    expect(base64(new ArrayBuffer(0))).toBe('');
  });

  it('imports through document.import_data, and refuses unknown files', async () => {
    const s = new FakeService();
    const svg = '<svg xmlns="http://www.w3.org/2000/svg" height="10"><rect width="5" height="5"/></svg>';
    const r = await importFile(s, 'a.svg', new TextEncoder().encode(svg).buffer as ArrayBuffer, 'keep');
    expect(r.objects).toHaveLength(1);
    await expect(importFile(s, 'a.nc', new ArrayBuffer(1), 'keep')).rejects.toThrow('cannot import a.nc');
  });
});
