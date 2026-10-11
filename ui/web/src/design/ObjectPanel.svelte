<script lang="ts">
  // The inspector of the selection (design/design-system.md "Objeto"): position by
  // the centre, size, rotation, text, layer, align, duplicate, delete. Each edit is
  // one `document.*` action; "keep proportions" is a local preference of the panel.
  import type { AlignHow, TransformOp } from '../contract/document';
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import type { DocumentProjection } from '../state/document.svelte';
  import { PALETTE_COLORS, assignColor, layerCode } from './layers';
  import NumField from './NumField.svelte';
  import { rotationOf, selectionBox } from './transform';

  let { doc }: { doc: DocumentProjection } = $props();
  const { run, settings } = app();
  let lock = $state(true);

  const sel = $derived(doc.selected);
  const ids = $derived(sel.map((o) => o.id));
  const box = $derived(selectionBox(sel.map((o) => o.bounds)));
  const cx = $derived(box ? box.x + box.width / 2 : 0);
  const cy = $derived(box ? box.y + box.height / 2 : 0);
  const rot = $derived(sel.length === 1 ? rotationOf(sel[0]!.transform) : 0);
  const text = $derived(sel.length === 1 && sel[0]!.kind === 'text' ? sel[0]! : null);
  const inch = $derived(settings.ui.units === 'in');
  const lenUnit = $derived(inch ? 'in' : 'mm');
  const lenScale = $derived(inch ? 25.4 : 1);
  const usedColors = $derived(
    new Set(sel.map((o) => doc.layers.find((l) => l.id === o.layer)?.color.toUpperCase() ?? '')),
  );

  const transform = (op: TransformOp) => run(() => doc.service.dispatch('document.transform', { ids, op }));
  function size(axis: 'w' | 'h', v: number) {
    if (!box || v <= 0) return;
    const cur = axis === 'w' ? box.width : box.height;
    if (cur <= 0) return;
    const k = v / cur;
    const scale: [number, number] = lock ? [k, k] : axis === 'w' ? [k, 1] : [1, k];
    return transform({ scale, pivot: [cx, cy] });
  }
  const align = (how: AlignHow) => run(() => doc.service.dispatch('document.align', { ids, how }));
  const aligns: [
    AlignHow,
    'alignLeft' | 'alignCenterH' | 'alignRight' | 'alignTop' | 'alignMiddleV' | 'alignBottom',
  ][] = [
    ['left', 'alignLeft'],
    ['center_h', 'alignCenterH'],
    ['right', 'alignRight'],
    ['top', 'alignTop'],
    ['middle_v', 'alignMiddleV'],
    ['bottom', 'alignBottom'],
  ];
  const kindLabel = (k: string, name: string) =>
    k === 'text'
      ? t('obj-type-text')
      : k === 'image'
        ? t('obj-type-image')
        : name === 'rect'
          ? t('obj-type-rect')
          : name === 'ellipse'
            ? t('obj-type-ellipse')
            : t('obj-type-path');
</script>

<p class="panel-sub" data-testid="obj-sub">
  {sel.length === 1
    ? kindLabel(sel[0]!.kind, sel[0]!.name)
    : sel.length > 1
      ? t('obj-count', { n: sel.length })
      : ''}
</p>
<div class="inspector" data-testid="inspector">
  {#if !box}
    <p class="empty">{t('obj-none')}</p>
  {:else}
    <div class="fgrid">
      <NumField
        label="X"
        title={t('obj-center')}
        value={cx}
        unit={lenUnit}
        scale={lenScale}
        testid="obj-x"
        oncommit={(v) => transform({ translate: [v - cx, 0] })}
      />
      <NumField
        label="Y"
        title={t('obj-center')}
        value={cy}
        unit={lenUnit}
        scale={lenScale}
        testid="obj-y"
        oncommit={(v) => transform({ translate: [0, v - cy] })}
      />
      <NumField
        label={t('obj-w')}
        value={box.width}
        unit={lenUnit}
        scale={lenScale}
        min={0.1}
        testid="obj-w"
        oncommit={(v) => size('w', v)}
      />
      <NumField
        label={t('obj-h')}
        value={box.height}
        unit={lenUnit}
        scale={lenScale}
        min={0.1}
        testid="obj-h"
        oncommit={(v) => size('h', v)}
      />
      <NumField
        label={t('obj-rot')}
        value={rot}
        unit="°"
        decimals={1}
        testid="obj-rot"
        oncommit={(v) => transform({ rotate_deg: v - rot, pivot: [cx, cy] })}
      />
    </div>
    <label class="check"
      ><input type="checkbox" bind:checked={lock} data-testid="obj-lock" /><Icon name="lock" /><span
        >{t('obj-lock')}</span
      ></label
    >

    {#if text?.text}
      <div class="field">
        <label for="obj-text">{t('obj-text')}</label>
        <textarea
          id="obj-text"
          class="textarea"
          rows="2"
          value={text.text.text}
          onchange={(e) =>
            run(() =>
              doc.service.dispatch('document.set_text', { id: text.id, text: e.currentTarget.value || ' ' }),
            )}></textarea>
      </div>
      <NumField
        label={t('obj-size')}
        value={text.text.height_mm}
        unit={lenUnit}
        scale={lenScale}
        min={0.5}
        oncommit={(v) => run(() => doc.service.dispatch('document.set_text', { id: text.id, height_mm: v }))}
      />
    {/if}

    <div class="field">
      <span class="mini">{t('obj-layer')}</span>
      <div class="btn-row" role="group" aria-label={t('obj-layer')}>
        {#each PALETTE_COLORS as color (color)}
          <button
            class="icon-btn swatch"
            style="--c:{color}"
            class:used={usedColors.has(color)}
            title={layerCode(color)}
            aria-label={layerCode(color)}
            aria-pressed={usedColors.has(color)}
            onclick={() => run(() => assignColor(doc, color))}>{layerCode(color).slice(1)}</button
          >
        {/each}
      </div>
    </div>

    <div class="field">
      <span class="mini">{t('obj-align')}</span>
      <div class="btn-row" role="group" aria-label={t('obj-align')}>
        {#each aligns as [how, icon] (how)}
          <button
            class="icon-btn"
            title={t(`obj-${icon}`)}
            aria-label={t(`obj-${icon}`)}
            disabled={sel.length < 2}
            onclick={() => align(how)}><Icon name={icon} /></button
          >
        {/each}
      </div>
    </div>

    <div class="btn-row">
      <button
        class="btn"
        title={t('obj-duplicate')}
        data-testid="obj-duplicate"
        onclick={() => run(() => doc.service.dispatch('document.duplicate', { ids }))}
        ><Icon name="copy" /><span>{t('obj-duplicate').replace(/\s*\(.*\)$/, '')}</span></button
      >
      <button
        class="btn"
        title={t('obj-delete')}
        data-testid="obj-delete"
        onclick={() => run(() => doc.service.dispatch('document.delete', { ids }))}
        ><Icon name="trash" /><span>{t('obj-delete').replace(/\s*\(.*\)$/, '')}</span></button
      >
    </div>
  {/if}
</div>

<style>
  .swatch {
    background: var(--c);
    border: 2px solid transparent;
    color: #000c;
    font: 700 10px var(--fk-font-family-mono);
  }
  .swatch.used {
    border-color: var(--fk-color-text);
  }
</style>
