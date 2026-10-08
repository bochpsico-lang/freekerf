<script lang="ts">
  // The colour palette over the canvas (design/ux.md "Cor = camada"): picking a
  // colour moves the selection to that colour's layer, or, with nothing selected,
  // makes it the layer of new shapes.
  import type { DocumentProjection } from '../state/document.svelte';
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import { PALETTE_COLORS, assignColor, layerCode } from './layers';

  let { doc }: { doc: DocumentProjection } = $props();
  const { run } = app();

  const current = $derived(doc.layers.find((l) => l.id === doc.summary?.current_layer)?.color.toUpperCase());
  const used = $derived(
    new Set(doc.selected.map((o) => doc.layers.find((l) => l.id === o.layer)?.color.toUpperCase() ?? '')),
  );
</script>

<div class="palette" role="toolbar" aria-label={t('obj-layer')} data-testid="palette">
  {#each PALETTE_COLORS as color (color)}
    <button
      class="sw"
      class:cur={current === color}
      style="--c:{color}"
      title="{layerCode(color)} — {t('palette-hint')}"
      aria-label={layerCode(color)}
      aria-pressed={used.has(color)}
      data-color={color}
      onclick={() => run(() => assignColor(doc, color))}>{layerCode(color).slice(1)}</button
    >
  {/each}
</div>
