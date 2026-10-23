<script lang="ts">
  // The dock (wide layout), drawer (medium) or bottom area (narrow) with the panels.
  // Panel contents arrive with their PRs. The tabs follow the mode: the layers and
  // the inspector in Design, the machine and the console in Produce (the view moves
  // the panel when the mode changes); every panel stays mounted, hidden.
  // Wide layout: its left edge is dragged to change its width and, in Design mode,
  // the separator under Layers to change their height (`ui.dock_*`, dock-size.ts).
  import type { Snippet } from 'svelte';

  import { app } from '../context';
  import type { Panel } from '../gen/api';
  import type { IconName } from '../gen/icons';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import {
    DOCK_MAX_SHARE,
    DOCK_MIN_PX,
    LAYERS_MAX_PCT,
    LAYERS_MIN_PCT,
    clampLayers,
    keyResize,
    layersAt,
    widthAt,
  } from './dock-size';

  let { panels }: { panels: Partial<Record<Panel, Snippet>> } = $props();
  const { view, service, run, env, settings } = app();
  const tabs: [Panel, IconName][] = [
    ['layers', 'layers'],
    ['object', 'sliders'],
    ['machine', 'machine'],
    ['console', 'console'],
  ];
  // the narrow layout is machine-only (App.svelte)
  const produce = $derived(env.layout === 'narrow' || view.mode === 'produce');
  const shownTabs = $derived(tabs.filter(([p]) => (p === 'machine' || p === 'console') === produce));
  const open = $derived(env.layout !== 'medium' || view.mode === 'produce' || !!view.state?.drawer_open);

  type Kind = 'width' | 'layers';
  const KEYS = { width: 'ui.dock_width_px', layers: 'ui.dock_layers_pct' } as const;
  let dock: HTMLElement | undefined = $state();
  /** The size being dragged, or set here and not yet back from the settings (0: default). */
  let live: Record<Kind, number | null> = $state({ width: null, layers: null });
  let dragging: Kind | null = $state(null);

  const wide = $derived(env.layout === 'wide');
  const split = $derived(wide && view.mode === 'design');
  const width = $derived(live.width ?? settings.ui.dock_width_px);
  const layers = $derived(live.layers ?? settings.ui.dock_layers_pct);

  /** The size on screen, the default one included. */
  function shown(kind: Kind): number {
    const box = dock?.getBoundingClientRect();
    if (!box) return 0;
    if (kind === 'width') return Math.round(box.width);
    const h = dock?.querySelector('#dock-panel-layers')?.getBoundingClientRect().height ?? 0;
    return box.height > 0 ? clampLayers((h / box.height) * 100) : LAYERS_MIN_PCT;
  }

  function sizeAt(kind: Kind, e: PointerEvent): number | null {
    const box = dock?.getBoundingClientRect();
    if (!box) return null;
    return kind === 'width'
      ? widthAt(e.clientX, box.right, window.innerWidth)
      : layersAt(e.clientY, box.top, box.bottom);
  }

  function save(kind: Kind, value: number): Promise<void> {
    live[kind] = value;
    return run(() => service.dispatch('settings.set', { key: KEYS[kind], value }));
  }

  function down(kind: Kind, e: PointerEvent): void {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    dragging = kind;
  }

  function move(kind: Kind, e: PointerEvent): void {
    if (dragging !== kind) return;
    const v = sizeAt(kind, e);
    if (v !== null) live[kind] = v;
  }

  function up(kind: Kind): void {
    if (dragging !== kind) return;
    dragging = null;
    const v = live[kind];
    if (v !== null) void save(kind, v);
  }

  function key(kind: Kind, e: KeyboardEvent): void {
    const current = (kind === 'width' ? width : layers) || shown(kind);
    const v = keyResize(kind, e.key, e.shiftKey, current, window.innerWidth);
    if (v === null) return;
    e.preventDefault();
    void save(kind, v);
  }
</script>

<aside
  class="dock"
  class:open
  class:resizing={dragging !== null}
  aria-label={t('drawer-toggle')}
  data-testid="dock"
  bind:this={dock}
  style:width={wide && width ? `clamp(${DOCK_MIN_PX}px, ${width}px, ${DOCK_MAX_SHARE * 100}vw)` : null}
>
  {#if wide}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (a focusable separator is a widget, WAI-ARIA "Window Splitter") -->
    <div
      class="dock-resize width"
      role="separator"
      aria-orientation="vertical"
      aria-label={t('panel-resize-width')}
      aria-valuemin={DOCK_MIN_PX}
      aria-valuenow={width || undefined}
      title={t('panel-resize-width')}
      tabindex="0"
      data-testid="dock-resize-width"
      onpointerdown={(e) => down('width', e)}
      onpointermove={(e) => move('width', e)}
      onpointerup={() => up('width')}
      onpointercancel={() => up('width')}
      ondblclick={() => save('width', 0)}
      onkeydown={(e) => key('width', e)}
    ></div>
  {/if}
  <div class="dock-tabs" role="tablist">
    {#each shownTabs as [p, icon] (p)}
      <button
        role="tab"
        data-panel={p}
        aria-selected={view.panel === p}
        title={t(`panel-${p}`)}
        onclick={() => run(() => service.dispatch('view.open_panel', { name: p }))}
      >
        <Icon name={icon} /><span>{t(`panel-${p}`)}</span>
      </button>
    {/each}
    <button
      class="icon-btn dock-close"
      title={t('panel-close')}
      aria-label={t('panel-close')}
      onclick={() => run(() => service.dispatch('view.close_panel', { name: view.panel }))}
    >
      <Icon name="close" />
    </button>
  </div>
  {#each tabs as [p, icon] (p)}
    <section
      class="panel"
      class:show={view.panel === p}
      class:sized={p === 'layers' && split && !!layers}
      data-panel={p}
      id={`dock-panel-${p}`}
      aria-label={t(`panel-${p}`)}
      style:flex-basis={p === 'layers' && split && layers ? `${layers}%` : null}
    >
      <header class="panel-h"><h2><Icon name={icon} /><span>{t(`panel-${p}`)}</span></h2></header>
      {#if panels[p]}
        {@render panels[p]()}
      {:else}
        <p class="placeholder">—</p>
      {/if}
    </section>
  {/each}
  {#if split}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (a focusable separator is a widget, WAI-ARIA "Window Splitter") -->
    <div
      class="dock-resize layers"
      role="separator"
      aria-orientation="horizontal"
      aria-controls="dock-panel-layers"
      aria-label={t('panel-resize-layers')}
      aria-valuemin={LAYERS_MIN_PCT}
      aria-valuemax={LAYERS_MAX_PCT}
      aria-valuenow={layers || undefined}
      title={t('panel-resize-layers')}
      tabindex="0"
      data-testid="dock-resize-layers"
      onpointerdown={(e) => down('layers', e)}
      onpointermove={(e) => move('layers', e)}
      onpointerup={() => up('layers')}
      onpointercancel={() => up('layers')}
      ondblclick={() => save('layers', 0)}
      onkeydown={(e) => key('layers', e)}
    ></div>
  {/if}
</aside>
