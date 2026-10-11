// The Design mode in the real app, on the real document: draw with a tool, import
// an SVG through the dialog, change a layer with the palette, undo. The document is
// read back through the app's own `session.snapshot`.
import { $, browser, expect } from '@wdio/globals';

import { byTestId } from './helpers';

interface Snapshot {
  selection: number[];
  undo: string[];
  document: {
    layers: { id: number; color: string }[];
    objects: { id: number; layer: number; kind: string }[];
  };
}

/** The app's session snapshot, read with the window's own `dispatch`. */
const snapshot = () =>
  browser.execute(async () => {
    const w = window as unknown as {
      __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
    };
    return (await w.__TAURI_INTERNALS__.invoke('dispatch', {
      action: 'session.snapshot',
      params: {},
    })) as Snapshot;
  });

const SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20">' +
  '<rect x="1" y="1" width="10" height="10" stroke="#E8622C" fill="none"/>' +
  '<circle cx="25" cy="10" r="5" stroke="#10A37F" fill="none"/></svg>';

describe('Design mode on the real document', () => {
  it('starts in the Design mode and draws a rectangle with the tool', async () => {
    await byTestId('state-chip').waitForDisplayed({ timeout: 30_000 });
    await byTestId('mode-design').click();
    await expect($('html')).toHaveAttribute('data-mode', 'design');
    await $('[data-tool="rect"]').click();
    const canvas = await byTestId('canvas');
    await canvas.click();
    await browser.waitUntil(async () => (await snapshot()).document.objects.length === 1, {
      timeout: 10_000,
    });
    const s = await snapshot();
    expect(s.document.objects[0]!.kind).toBe('shape');
    expect(s.selection).toEqual([s.document.objects[0]!.id]);
    expect(s.undo).toEqual(['add shape']);
  });

  it('imports an SVG by colour layer through the dialog', async () => {
    await $('[data-tool="import"]').click();
    const input = await byTestId('import-file');
    await browser.execute(
      (el, text) => {
        const i = el as unknown as HTMLInputElement;
        const dt = new DataTransfer();
        dt.items.add(new File([text], 'logo.svg', { type: 'image/svg+xml' }));
        i.files = dt.files;
        i.dispatchEvent(new Event('change', { bubbles: true }));
      },
      input,
      SVG,
    );
    await byTestId('import-go').click();
    await browser.waitUntil(async () => (await snapshot()).document.objects.length === 3, {
      timeout: 10_000,
    });
    const colors = (await snapshot()).document.layers.map((l) => l.color);
    expect(colors).toEqual(expect.arrayContaining(['#E8622C', '#10A37F']));
  });

  it('the palette moves the selection to a colour layer, and undo puts it back', async () => {
    const before = await snapshot();
    const id = before.document.objects[0]!.id;
    await browser.execute(
      async (ids) => {
        const w = window as unknown as {
          __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        };
        await w.__TAURI_INTERNALS__.invoke('dispatch', { action: 'document.select', params: { ids } });
      },
      [id],
    );
    await $('.palette [data-color="#B36AE2"]').click();
    const layerOf = async () => {
      const s = await snapshot();
      const o = s.document.objects.find((x) => x.id === id)!;
      return s.document.layers.find((l) => l.id === o.layer)!.color;
    };
    await browser.waitUntil(async () => (await layerOf()) === '#B36AE2', { timeout: 10_000 });
    await $('[data-tool="undo"]').click();
    await browser.waitUntil(async () => (await layerOf()) !== '#B36AE2', { timeout: 10_000 });
  });

  it('imports a photo and sets its engraving in the raster dialog (Rust preview)', async () => {
    await $('[data-tool="import"]').click();
    const input = await byTestId('import-file');
    // a 40 × 20 grey ramp drawn with a canvas, as a PNG file
    await browser.execute(async (el) => {
      const c = document.createElement('canvas');
      c.width = 40;
      c.height = 20;
      const g = c.getContext('2d')!;
      for (let x = 0; x < 40; x++) {
        g.fillStyle = `rgb(${x * 6},${x * 6},${x * 6})`;
        g.fillRect(x, 0, 1, 20);
      }
      const blob = await new Promise<Blob>((r) => c.toBlob((b) => r(b!), 'image/png'));
      const dt = new DataTransfer();
      dt.items.add(new File([blob], 'ramp.png', { type: 'image/png' }));
      const i = el as unknown as HTMLInputElement;
      i.files = dt.files;
      i.dispatchEvent(new Event('change', { bubbles: true }));
    }, input);
    await byTestId('import-go').click();
    await browser.waitUntil(async () => (await snapshot()).document.objects.some((o) => o.kind === 'image'), {
      timeout: 10_000,
    });
    const image = (await snapshot()).document.objects.find((o) => o.kind === 'image')!;
    await browser.execute(
      async (ids) => {
        const w = window as unknown as {
          __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        };
        await w.__TAURI_INTERNALS__.invoke('dispatch', { action: 'document.select', params: { ids } });
        await w.__TAURI_INTERNALS__.invoke('dispatch', {
          action: 'view.open_panel',
          params: { name: 'object' },
        });
      },
      [image.id],
    );
    await byTestId('obj-image-open').click();
    await expect(byTestId('raster-preview')).toBeDisplayed({ wait: 10_000 });
    await $('[data-tool-mode="dithering"]').click();
    await byTestId('raster-apply').click();
    await browser.waitUntil(
      async () => {
        const s = (await snapshot()) as unknown as {
          document: { objects: { id: number; image?: { tool: string } }[] };
        };
        return s.document.objects.find((o) => o.id === image.id)?.image?.tool === 'dithering';
      },
      { timeout: 10_000 },
    );
  });
});
