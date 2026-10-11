<script lang="ts">
  // The raster dialog (design/ux.md flow 4): how the selected image is engraved,
  // with a preview of the bitmap the settings make (`document.raster_preview`,
  // computed in Rust). Nothing changes until Apply (`document.set_raster`, one undo
  // step). The settings being edited are local to the dialog.
  import { app } from '../context';
  import type { DitherMode, LineDirection, RasterSettings, RasterTool } from '../gen/api';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import DialogFrame from '../shell/DialogFrame.svelte';
  import NumField from './NumField.svelte';

  let { onclose }: { onclose: () => void } = $props();
  const { doc, service, run } = app();

  const TOOLS: RasterTool[] = ['line_to_line', 'dithering', 'passthrough', 'vectorize'];
  const DITHERS: [DitherMode, string][] = [
    ['floyd_steinberg', 'Floyd–Steinberg'],
    ['atkinson', 'Atkinson'],
    ['burks', 'Burkes'],
    ['jarvis', 'Jarvis–Judice–Ninke'],
    ['sierra2', 'Sierra 2'],
    ['sierra3', 'Sierra 3'],
    ['sierra_light', 'Sierra Lite'],
    ['stucki', 'Stucki'],
    ['random', ''],
  ];
  const DIRECTIONS: LineDirection[] = ['horizontal', 'vertical', 'diagonal'];
  /** Wait this long after the last change before computing the preview, ms. */
  const PREVIEW_DELAY_MS = 250;

  const target = $derived(
    doc.selected.length === 1 && doc.selected[0]!.kind === 'image' ? doc.selected[0]! : null,
  );
  let settings: RasterSettings | null = $state(null);
  let png = $state('');
  let size = $state('');
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const id = target?.id;
    if (id === undefined) return;
    void run(async () => {
      settings = await service.dispatch('document.raster_settings', { id });
      await preview();
    });
  });

  async function preview() {
    const id = target?.id;
    if (id === undefined || !settings) return;
    const p = await service.dispatch('document.raster_preview', { id, settings: $state.snapshot(settings) });
    png = `data:image/png;base64,${p.png}`;
    size = `${p.width} × ${p.height} px`;
  }

  /** Changes a setting and recomputes the preview after a pause. */
  function change(f: (s: RasterSettings) => void) {
    if (!settings) return;
    f(settings);
    clearTimeout(timer);
    timer = setTimeout(() => void run(preview), PREVIEW_DELAY_MS);
  }

  async function apply() {
    const id = target?.id;
    if (id === undefined || !settings) return;
    const s = $state.snapshot(settings);
    await run(async () => {
      await service.dispatch('document.set_raster', { id, settings: s });
      onclose();
    });
  }
</script>

<DialogFrame title={t('raster-title')} testid="dialog-raster" wide {onclose}>
  {#if !target || !settings}
    <p class="hint" data-testid="raster-none">{t('raster-none')}</p>
  {:else}
    {@const s = settings}
    <div class="raster">
      <div class="controls">
        <div class="field">
          <span class="mini">{t('raster-tool')}</span>
          <div class="seg" role="radiogroup" aria-label={t('raster-tool')}>
            {#each TOOLS as tool (tool)}
              <button
                role="radio"
                aria-checked={s.tool === tool}
                data-tool-mode={tool}
                onclick={() => change((x) => (x.tool = tool))}>{t(`raster-tool-${tool}`)}</button
              >
            {/each}
          </div>
        </div>
        {#if s.tool === 'dithering'}
          <div class="field">
            <label for="raster-dither">{t('obj-dither')}</label>
            <select
              id="raster-dither"
              data-testid="raster-dither"
              value={s.dithering}
              onchange={(e) => change((x) => (x.dithering = e.currentTarget.value as DitherMode))}
            >
              {#each DITHERS as [mode, name] (mode)}
                <option value={mode}>{name || t('raster-dither-random')}</option>
              {/each}
            </select>
          </div>
        {/if}
        <div class="fgrid">
          <NumField
            label={t('raster-lines')}
            value={s.lines_per_mm}
            unit="/mm"
            min={0.1}
            max={20}
            testid="raster-lines"
            oncommit={(v) => change((x) => (x.lines_per_mm = v))}
          />
          <NumField
            label={t('obj-brightness')}
            value={s.adjustments.brightness}
            unit="%"
            decimals={0}
            min={0}
            max={200}
            testid="raster-brightness"
            oncommit={(v) => change((x) => (x.adjustments.brightness = Math.round(v)))}
          />
          <NumField
            label={t('obj-contrast')}
            value={s.adjustments.contrast}
            unit="%"
            decimals={0}
            min={0}
            max={200}
            testid="raster-contrast"
            oncommit={(v) => change((x) => (x.adjustments.contrast = Math.round(v)))}
          />
          <NumField
            label={t('raster-speed')}
            value={s.mark_speed_mm_min}
            unit="mm/min"
            decimals={0}
            min={1}
            oncommit={(v) => change((x) => (x.mark_speed_mm_min = Math.round(v)))}
          />
          <NumField
            label={t('raster-minPower')}
            value={s.min_power}
            decimals={0}
            min={0}
            oncommit={(v) => change((x) => (x.min_power = Math.round(v)))}
          />
          <NumField
            label={t('raster-maxPower')}
            value={s.max_power}
            decimals={0}
            min={0}
            oncommit={(v) => change((x) => (x.max_power = Math.round(v)))}
          />
        </div>
        <div class="field">
          <span class="mini">{t('raster-direction')}</span>
          <div class="seg" role="radiogroup" aria-label={t('raster-direction')}>
            {#each DIRECTIONS as d (d)}
              <button
                role="radio"
                aria-checked={s.direction === d}
                onclick={() => change((x) => (x.direction = d))}>{t(`raster-dir-${d}`)}</button
              >
            {/each}
          </div>
        </div>
        <label class="check"
          ><input
            type="checkbox"
            data-testid="raster-threshold"
            checked={s.adjustments.threshold != null}
            onchange={(e) => change((x) => (x.adjustments.threshold = e.currentTarget.checked ? 50 : null))}
          /><span>{t('obj-dither-thr')}</span></label
        >
      </div>
      <figure class="preview">
        {#if png}<img src={png} alt={t('raster-preview')} data-testid="raster-preview" />{/if}
        <figcaption class="hint mono">{size}</figcaption>
      </figure>
    </div>
  {/if}
  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t('preflight-cancel')}</button>
    <button class="btn primary" disabled={!target || !settings} data-testid="raster-apply" onclick={apply}
      ><Icon name="check" /><span>{t('raster-apply')}</span></button
    >
  {/snippet}
</DialogFrame>

<style>
  .raster {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--fk-space-4);
  }
  .controls {
    display: grid;
    gap: var(--fk-space-3);
    align-content: start;
  }
  .preview {
    margin: 0;
    display: grid;
    gap: var(--fk-space-2);
    align-content: start;
  }
  .preview img {
    width: 100%;
    image-rendering: pixelated;
    background: #fff;
    border: 1px solid var(--fk-color-border);
  }
  @media (max-width: 720px) {
    .raster {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
