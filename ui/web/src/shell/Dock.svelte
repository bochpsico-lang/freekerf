<script lang="ts">
  // The dock (wide layout), drawer (medium) or bottom area (narrow) with the panels.
  // Panel contents arrive with their PRs; Machine is the Produce mode panel.
  import type { Snippet } from 'svelte';

  import { app } from '../context';
  import type { Panel } from '../gen/api';
  import type { IconName } from '../gen/icons';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';

  let { panels }: { panels: Partial<Record<Panel, Snippet>> } = $props();
  const { view, service, run, env } = app();
  const tabs: [Panel, IconName][] = [
    ['layers', 'layers'],
    ['object', 'sliders'],
    ['machine', 'machine'],
    ['console', 'console'],
  ];
  const open = $derived(env.layout !== 'medium' || view.mode === 'produce' || !!view.state?.drawer_open);
</script>

<aside class="dock" class:open aria-label={t('drawer-toggle')} data-testid="dock">
  <div class="dock-tabs" role="tablist">
    {#each tabs as [p, icon] (p)}
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
    <section class="panel" class:show={view.panel === p} data-panel={p} aria-label={t(`panel-${p}`)}>
      <header class="panel-h"><h2><Icon name={icon} /><span>{t(`panel-${p}`)}</span></h2></header>
      {#if panels[p]}
        {@render panels[p]()}
      {:else}
        <p class="placeholder">—</p>
      {/if}
    </section>
  {/each}
</aside>
