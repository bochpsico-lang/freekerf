<script lang="ts">
  // The layers panel (design/design-system.md "Camadas"): one row per colour layer
  // in output order; expand to edit mode, speed, power and passes; show or hide;
  // move in the output order. Every change is one `document.*` action.
  import type { LayerMode } from '../gen/api';
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import type { DocumentProjection, Layer } from '../state/document.svelte';
  import { MODES, layerCode, layerParams, modeLabel } from './layers';
  import NumField from './NumField.svelte';

  let { doc }: { doc: DocumentProjection } = $props();
  const { run, machine } = app();
  /** Expanded rows: local, purely visual. */
  let open: Record<number, boolean> = $state({});
  const maxPower = $derived(machine.info?.max_power || 1000);

  const count = (l: Layer) => l.objects;
  const update = (
    id: number,
    patch: Partial<Pick<Layer, 'mode' | 'speed_mm_min' | 'power' | 'passes' | 'visible'>>,
  ) => run(() => doc.service.dispatch('document.update_layer', { id, ...patch }));
  const current = (id: number) => run(() => doc.service.dispatch('document.set_current_layer', { id }));
  const move = (id: number, to: number) => run(() => doc.service.dispatch('document.move_layer', { id, to }));
</script>

<p class="panel-sub layers-order">{t('layers-order')}</p>
<div class="layer-list" role="list" data-testid="layer-list">
  {#each doc.layers as l, i (l.id)}
    <div
      class="layer"
      class:open={open[l.id]}
      class:cur={doc.summary?.current_layer === l.id}
      role="listitem"
      style="--c:{l.color}"
      data-layer={l.id}
    >
      <!-- the row selects the layer for new shapes; the chevron expands it -->
      <div class="layer-row" role="presentation" onclick={() => current(l.id)}>
        <span class="layer-chip">{layerCode(l.color).replace('C', '')}</span>
        <span class="layer-main">
          <span class="layer-mode"
            >{l.name || t(modeLabel(l.mode))} · {t('layers-objects', { n: count(l) })}</span
          >
          <span class="layer-params">{layerParams(l, maxPower)}</span>
        </span>
        <span></span>
        <button
          class="icon-btn"
          aria-pressed={l.visible}
          title={t('layers-show')}
          aria-label="{t('layers-show')} {layerCode(l.color)}"
          onclick={(e) => {
            e.stopPropagation();
            void update(l.id, { visible: !l.visible });
          }}><Icon name={l.visible ? 'eye' : 'eyeOff'} /></button
        >
        <button
          class="icon-btn exp"
          title={t('layers-edit')}
          aria-label={t('layers-edit')}
          aria-expanded={!!open[l.id]}
          onclick={(e) => {
            e.stopPropagation();
            open[l.id] = !open[l.id];
          }}><Icon name="chevDown" /></button
        >
      </div>
      {#if open[l.id]}
        <div class="layer-edit">
          <div class="seg wrap" role="radiogroup" aria-label={t('layers-mode')}>
            {#each MODES as m (m)}
              <button
                role="radio"
                aria-checked={l.mode === m}
                class:on={l.mode === m}
                onclick={() => update(l.id, { mode: m as LayerMode })}><span>{t(modeLabel(m))}</span></button
              >
            {/each}
          </div>
          <div class="row2">
            <div class="field">
              <span class="mini">{t('layers-speed')}</span>
              <NumField
                bare
                label={t('layers-speed')}
                value={l.speed_mm_min}
                unit="mm/min"
                decimals={0}
                min={1}
                testid="layer-speed"
                oncommit={(v) => update(l.id, { speed_mm_min: Math.round(v) })}
              />
            </div>
            <div class="field">
              <span class="mini">{t('layers-power')}</span>
              <NumField
                bare
                label={t('layers-power')}
                value={(100 * l.power) / maxPower}
                unit="%"
                decimals={0}
                min={0}
                max={100}
                testid="layer-power"
                oncommit={(v) => update(l.id, { power: Math.round((v * maxPower) / 100) })}
              />
            </div>
            <div class="field">
              <span class="mini">{t('layers-passes')}</span>
              <NumField
                bare
                label={t('layers-passes')}
                value={l.passes}
                decimals={0}
                min={1}
                max={50}
                oncommit={(v) => update(l.id, { passes: Math.round(v) })}
              />
            </div>
          </div>
          <div class="btn-row">
            <button
              class="icon-btn"
              title={t('layers-up')}
              aria-label={t('layers-up')}
              disabled={i === 0}
              onclick={() => move(l.id, i - 1)}><Icon name="up" /></button
            >
            <button
              class="icon-btn"
              title={t('layers-down')}
              aria-label={t('layers-down')}
              disabled={i === doc.layers.length - 1}
              onclick={() => move(l.id, i + 1)}><Icon name="down" /></button
            >
          </div>
        </div>
      {/if}
    </div>
  {:else}
    <p class="empty">{t('layers-empty')}</p>
  {/each}
</div>
