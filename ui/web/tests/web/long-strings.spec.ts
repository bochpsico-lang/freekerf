// Long strings (German is 30-40 % longer than English): no control of the shell
// may cut its text or push the layout sideways, in the three layouts
// (design/interaction.md §7: labels wrap; tabs and table rows use ellipsis + title).
import { expect, test, type Page } from '@playwright/test';

const WIDTHS = [1366, 1000, 400];

/** Elements whose text overflows them without an ellipsis allowed by the design. */
async function clipped(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const allowed = (el: Element) =>
      getComputedStyle(el).textOverflow === 'ellipsis' || el.closest('[role="tab"], .file, .layer-params');
    for (const el of document.querySelectorAll<HTMLElement>(
      'button, label, h2, h3, dt, .hint, .state-label, span',
    )) {
      if (!el.offsetParent || el.closest('[hidden]')) continue;
      if (el.scrollWidth > el.clientWidth + 1 && !allowed(el)) {
        out.push(`${el.tagName.toLowerCase()}.${el.className}: "${el.textContent?.trim().slice(0, 40)}"`);
      }
    }
    if (document.documentElement.scrollWidth > window.innerWidth + 1) out.push('page scrolls sideways');
    return out;
  });
}

async function openInGerman(page: Page, width: number) {
  await page.setViewportSize({ width, height: 900 });
  await page.goto('/dev.html');
  await page.getByTestId('state-chip').waitFor();
  await page.getByTestId('open-settings').click();
  await page.getByTestId('language').selectOption('de');
  await expect(page.locator('html')).toHaveAttribute('lang', 'de');
  await page.keyboard.press('Escape');
}

for (const width of WIDTHS) {
  test(`German fits the shell at ${width} px`, async ({ page }) => {
    await openInGerman(page, width);
    await page.getByTestId('connect').click();
    await expect(page.getByTestId('state-chip')).toContainText('Betriebsbereit');
    expect(await clipped(page)).toEqual([]);
    // dialogs too
    await page.getByTestId('open-settings').click();
    for (const tab of ['display', 'machine', 'job', 'safety', 'automation']) {
      await page.getByTestId(`settings-tab-${tab}`).click();
      expect(await clipped(page), tab).toEqual([]);
    }
  });
}

test('the check catches clipped text', async ({ page }) => {
  await page.goto('/dev.html');
  await page.getByTestId('state-chip').waitFor();
  await page.evaluate(() => {
    const b = document.createElement('button');
    b.textContent = 'Werkstücknullpunkt wiederherstellen';
    Object.assign(b.style, { width: '40px', whiteSpace: 'nowrap', overflow: 'hidden' });
    document.querySelector('.topbar')?.appendChild(b);
  });
  expect((await clipped(page)).join()).toContain('Werkstücknullpunkt');
});
