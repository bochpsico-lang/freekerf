<script lang="ts">
  import { app } from '../context';
  import { flags } from '../flags';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import EStop from './EStop.svelte';
  import StateChip from './StateChip.svelte';

  const { machine, view, run, service } = app();
  const connected = $derived(machine.visual !== 'offline');
</script>

<header class="topbar">
  <div class="brand" aria-label="FreeKerf">
    <svg class="brand-mark" viewBox="0 0 64 64" aria-hidden="true"
      ><rect x="9" y="8" width="13" height="48" rx="2" fill="currentColor" /><path
        d="M43 8H57L39.5 32 57 56H43L25.5 32Z"
        fill="var(--fk-color-accent)"
      /><rect
        x="23"
        y="2"
        width="1.5"
        height="60"
        rx=".75"
        fill="var(--fk-color-accent)"
        opacity=".75"
      /></svg
    >
    <span class="brand-name">Free<b>Kerf</b></span>
  </div>
  <div class="file" title={machine.state?.program?.name ?? ''}>
    <span class="file-name">{machine.state?.program?.name ?? ''}</span>
  </div>
  <div class="seg mode-switch" role="tablist" aria-label="Mode">
    {#if flags.designMode}
      <button
        role="tab"
        aria-selected={view.mode === 'design'}
        data-testid="mode-design"
        onclick={() => run(() => service.dispatch('view.set_mode', { mode: 'design' }))}
      >
        <Icon name="pen" /><span>{t('mode-design')}</span>
      </button>
    {/if}
    <button
      role="tab"
      aria-selected={view.mode === 'produce'}
      data-testid="mode-produce"
      onclick={() => run(() => service.dispatch('view.set_mode', { mode: 'produce' }))}
    >
      <Icon name="laser" /><span>{t('mode-produce')}</span>
    </button>
  </div>
  <div class="top-spacer"></div>
  <StateChip />
  <button
    class="btn"
    id="btnConnectTop"
    data-testid="connect-top"
    onclick={() =>
      run(() =>
        connected
          ? service.dispatch('machine.disconnect', {})
          : service.dispatch('view.show_panel', { panel: 'machine' }),
      )}
  >
    <Icon name={connected ? 'plugOff' : 'plug'} />
    <span class="lbl">{t(connected ? 'machine-disconnect' : 'machine-connect')}</span>
  </button>
  <button
    class="icon-btn"
    title={t('settings-title')}
    aria-label={t('settings-title')}
    aria-haspopup="dialog"
    data-testid="open-settings"
    onclick={() => run(() => service.dispatch('view.open_dialog', { dialog: 'settings' }))}
  >
    <Icon name="gear" />
  </button>
  <EStop />
</header>
