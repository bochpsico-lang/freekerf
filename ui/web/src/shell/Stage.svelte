<script lang="ts">
  // The canvas area: canvas, zoom buttons, the large progress readout of the
  // Produce mode. The camera is local while a gesture runs; when it settles it is
  // committed to the view state (`view.set_viewport`), and viewports or fit
  // requests coming from the view state (scripts, agents) are applied here.
  import { onDestroy } from 'svelte';

  import Canvas from '../canvas/Canvas.svelte';
  import type { CameraState } from '../canvas/camera';
  import type { CanvasView } from '../canvas/view';
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import { formatDuration } from '../lib/format';

  let { onzoom }: { onzoom?: (realSize: number) => void } = $props();
  const { view, preview, machine, settings, service } = app();
  const workArea = { x: 0, y: 0, width: 400, height: 400 };
  /** CSS pixels per mm at 100 % (96 dpi). */
  const REAL_SIZE = 96 / 25.4;
  const COMMIT_MS = 250;

  let canvas: CanvasView | null = $state(null);
  let sent: { cx: number; cy: number; scale: number } | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let lastFit = -1;

  function camera(c: CameraState) {
    onzoom?.(c.scale / REAL_SIZE);
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      sent = { cx: c.cx, cy: c.cy, scale: c.scale };
      void service.dispatch('view.set_viewport', sent);
    }, COMMIT_MS);
  }

  const near = (a: number, b: number) => Math.abs(a - b) <= 1e-6 * Math.max(1, Math.abs(a), Math.abs(b));

  $effect(() => {
    const v = view.state?.viewport;
    if (!canvas || !v) return;
    if (sent && near(v.cx, sent.cx) && near(v.cy, sent.cy) && near(v.scale, sent.scale)) return;
    const c = canvas.camera;
    if (near(v.cx, c.cx) && near(v.cy, c.cy) && near(v.scale, c.scale)) return;
    sent = { ...v };
    canvas.setCamera({ ...c, cx: v.cx, cy: v.cy, scale: v.scale });
  });

  $effect(() => {
    const f = view.state?.fit_request ?? 0;
    if (!canvas) return;
    if (lastFit >= 0 && f !== lastFit) canvas.fit();
    lastFit = f;
  });

  // the canvas reads its colours from the tokens: read them again when the theme
  // changes (after App.svelte has updated the <html> attributes)
  $effect(() => {
    void settings.ui.theme;
    void settings.ui.high_contrast;
    const c = canvas;
    if (c) requestAnimationFrame(() => c.refreshTheme());
  });

  onDestroy(() => {
    if (timer) clearTimeout(timer);
  });

  /** Zoom by a factor around the centre (buttons and keys). */
  export function zoom(factor: number) {
    canvas?.zoom(factor);
  }

  const job = $derived(machine.state?.machine);
  const pct = $derived(
    job && job.in_program && job.job_target ? Math.floor((100 * job.job_executed) / job.job_target) : null,
  );
</script>

<section class="stage" id="stage" aria-label="Canvas" tabindex="-1">
  <Canvas
    {workArea}
    mode={view.mode}
    toolpath={preview.toolpath}
    head={preview.head}
    executed={preview.head?.executed ?? 0}
    units={settings.ui.units}
    oncamera={camera}
    bind:view={canvas}
  />
  {#if pct !== null}
    <div class="readout show" data-testid="readout">
      <div class="readout-pct">{pct}%</div>
      <div class="readout-sub">
        {t('machine-remaining')}
        {formatDuration(Math.max(0, (job?.projected_ms ?? 0) - (job?.job_time_ms ?? 0)))}
      </div>
    </div>
  {/if}
  <div class="canvas-tools" aria-label="View">
    <button
      class="icon-btn"
      title={t('view-zoomOut')}
      aria-label={t('view-zoomOut')}
      onclick={() => zoom(0.8)}><Icon name="zoomOut" /></button
    >
    <button
      class="icon-btn"
      title={t('view-fit')}
      aria-label={t('view-fit')}
      onclick={() => void service.dispatch('view.fit', {})}><Icon name="fit" /></button
    >
    <button class="icon-btn" title={t('view-zoomIn')} aria-label={t('view-zoomIn')} onclick={() => zoom(1.25)}
      ><Icon name="zoomIn" /></button
    >
  </div>
  <div class="narrow-note">{t('mode-narrowNote')}</div>
</section>
