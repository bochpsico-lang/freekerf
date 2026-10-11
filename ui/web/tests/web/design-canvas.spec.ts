// The Design mode gestures on the real canvas (WebGL2 in Chromium), on the fake
// document: drawing tools, selection, moving, resizing with a handle, marquee,
// undo. One `document.transform` per drag (doc/features/ui-contract.md).
import { expect, test, type Page } from '@playwright/test';

import type { FakeService } from '../../src/dev/fake-service';

type Win = { fkFake: FakeService };

const summary = (page: Page) =>
  page.evaluate(() => (window as unknown as Win).fkFake.document!.dispatch('document.get', {}));
const transforms = (page: Page) =>
  page.evaluate(() =>
    (window as unknown as Win).fkFake.document!.calls.filter((c) => c.action === 'document.transform'),
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
    .poll(() => page.evaluate(() => (window as unknown as Win).fkFake.view.viewport?.scale ?? 0))
    .toBeGreaterThan(0);
  const scale = await page.evaluate(() => (window as unknown as Win).fkFake.view.viewport!.scale);

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
