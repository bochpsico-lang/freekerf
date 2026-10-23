// The Design mode gestures on the real canvas (WebGL2 in Chromium), on the fake
// document: drawing tools, selection, moving, resizing with a handle, marquee,
// undo. One `document.transform` per drag (doc/features/design-mode.md).
import { expect, test, type Page } from '@playwright/test';

import type { FakeService } from '../../src/dev/fake-service';

type Win = { fkFake: FakeService };

const summary = (page: Page) =>
  page.evaluate(async () => {
    const s = await (window as unknown as Win).fkFake.dispatch('session.snapshot', {});
    return { selection: s.selection, layers: s.document.layers, objects: s.document.objects };
  });
const transforms = (page: Page) =>
  page.evaluate(() =>
    (window as unknown as Win).fkFake.calls.filter((c) => c.action === 'document.transform'),
  );

test('draw, select, move, resize and undo on the canvas', async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto('/dev.html');
  await page.getByTestId('mode-design').click();
  await expect(page.locator('html')).toHaveAttribute('data-mode', 'design');
  const canvas = page.getByTestId('canvas');
  const box = (await canvas.boundingBox())!;
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;

  // R, then a click: a 40 × 30 mm rectangle centred there, back to select
  await page.mouse.move(cx, cy);
  await page.keyboard.press('r');
  await page.mouse.click(cx, cy);
  await expect.poll(async () => (await summary(page)).objects.length).toBe(1);
  await expect.poll(() => page.evaluate(() => (window as unknown as Win).fkFake.view.tool)).toBe('select');
  const b0 = (await summary(page)).objects[0]!.bounds!;
  expect(b0[2] - b0[0]).toBeCloseTo(40);
  // screen pixels per mm, from the view state (committed after the gesture)
  await expect
    .poll(() => page.evaluate(() => (window as unknown as Win).fkFake.view.viewport?.zoom ?? 0))
    .toBeGreaterThan(0);
  const scale = await page.evaluate(() => (window as unknown as Win).fkFake.view.viewport!.zoom);

  // drag the object: one translate on release
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 50, cy, { steps: 5 });
  await page.mouse.move(cx + 100, cy, { steps: 5 });
  await page.mouse.up();
  await expect.poll(async () => (await transforms(page)).length).toBe(1);
  const [t1] = await transforms(page);
  expect((t1!.params as { op: { translate: [number, number] } }).op.translate[0]).toBeCloseTo(100 / scale, 0);

  // drag the south-east handle outwards: the box grows, proportions kept
  const b1 = (await summary(page)).objects[0]!.bounds!;
  const seX = cx + 100 + 20 * scale;
  const seY = cy + 15 * scale;
  await page.mouse.move(seX, seY);
  await page.mouse.down();
  await page.mouse.move(seX + 40, seY + 30, { steps: 8 });
  await page.mouse.up();
  await expect.poll(async () => (await transforms(page)).length).toBe(2);
  const b2 = (await summary(page)).objects[0]!.bounds!;
  expect(b2[2] - b2[0]).toBeGreaterThan(b1[2] - b1[0]);
  expect((b2[2] - b2[0]) / (b2[3] - b2[1])).toBeCloseTo(40 / 30, 1);

  // click on empty space clears the selection; a marquee selects again
  await page.mouse.click(box.x + 60, box.y + 60);
  await expect.poll(async () => (await summary(page)).selection.length).toBe(0);
  await page.mouse.move(box.x + 40, box.y + 40);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - 60, box.y + box.height - 60, { steps: 6 });
  await page.mouse.up();
  await expect.poll(async () => (await summary(page)).selection.length).toBe(1);

  // undo twice: back to the original rectangle
  await page.keyboard.press('Control+z');
  await page.keyboard.press('Control+z');
  await expect.poll(async () => (await summary(page)).objects[0]!.bounds).toEqual(b0);
});

test('the palette moves the selection to another layer', async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto('/dev.html');
  await page.getByTestId('mode-design').click();
  const box = (await page.getByTestId('canvas').boundingBox())!;
  await page.keyboard.press('e');
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect.poll(async () => (await summary(page)).selection.length).toBe(1);
  await page.locator('.palette [data-color="#E8622C"]').click();
  await expect
    .poll(async () => {
      const s = await summary(page);
      return s.layers.find((l) => l.id === s.objects[0]!.layer)?.color;
    })
    .toBe('#E8622C');
  await expect(page.getByTestId('layer-list').locator('.layer')).toHaveCount(2);
});

test('an imported image shows on the canvas as it will be engraved', async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto('/dev.html');
  await page.getByTestId('mode-design').click();
  await expect(page.locator('html')).toHaveAttribute('data-mode', 'design');
  // a black 200 × 150 mm PNG (the fake places it at 10 px/mm) moved to the orange layer
  await page.evaluate(async () => {
    const c = document.createElement('canvas');
    c.width = 2000;
    c.height = 1500;
    c.getContext('2d')!.fillRect(0, 0, c.width, c.height);
    const url = c.toDataURL('image/png');
    const fake = (window as unknown as Win).fkFake;
    const r = await fake.dispatch('document.import_data', {
      format: 'image',
      data: url.slice(url.indexOf(',') + 1),
      name: 'black.png',
    });
    const { layer } = await fake.dispatch('document.layer_for_color', { color: '#E8622C' });
    await fake.dispatch('document.move_to_layer', { ids: r.objects, layer });
  });
  const canvas = page.getByTestId('canvas');
  // the share of the canvas that is the layer's orange: the filled image, not only its frame
  const orange = async () => {
    const png = (await canvas.screenshot()).toString('base64');
    return page.evaluate(async (b64) => {
      const img = new Image();
      img.src = `data:image/png;base64,${b64}`;
      await img.decode();
      const c = document.createElement('canvas');
      c.width = img.width;
      c.height = img.height;
      const g = c.getContext('2d')!;
      g.drawImage(img, 0, 0);
      const d = g.getImageData(0, 0, c.width, c.height).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4)
        if (d[i]! > 200 && d[i + 1]! > 70 && d[i + 1]! < 130 && d[i + 2]! < 80) n++;
      return n / (d.length / 4);
    }, png);
  };
  await expect.poll(orange, { timeout: 10_000 }).toBeGreaterThan(0.05);
});
