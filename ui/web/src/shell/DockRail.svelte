<script lang="ts">
  // Medium layout: icons that open the panels as drawers.
  import { app } from '../context';
  import type { Panel } from '../gen/api';
  import type { IconName } from '../gen/icons';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';

  const { view, service, run } = app();
  const panels: [Panel, IconName][] = [
    ['layers', 'layers'],
    ['object', 'sliders'],
    ['machine', 'machine'],
    ['console', 'console'],
  ];

  function toggle(p: Panel) {
    const open = view.state?.drawer_open && view.panel === p;
    return run(() =>
      open
        ? service.dispatch('view.close_panel', { name: p })
        : service.dispatch('view.open_panel', { name: p }),
    );
  }
</script>

<nav class="dockrail" aria-label={t('drawer-toggle')}>
  {#each panels as [p, icon] (p)}
    <button
      class="icon-btn"
      title={t(`panel-${p}`)}
      aria-label={t(`panel-${p}`)}
      aria-pressed={!!view.state?.drawer_open && view.panel === p}
      onclick={() => toggle(p)}
    >
      <Icon name={icon} />
    </button>
  {/each}
</nav>
