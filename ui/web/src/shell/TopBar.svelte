<script lang="ts">
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import EStop from './EStop.svelte';
  import StateChip from './StateChip.svelte';

  const { machine, view, run, service, env, automation } = app();
  const agents = $derived(automation.status.agents.length);
  const script = $derived(automation.status.scripts[0] ?? null);
  const connected = $derived(machine.visual !== 'offline');
  // a job runs or is paused: the view stays in Produce
  const locked = $derived(!!view.state?.produce_locked);
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
    {#if env.layout !== 'narrow'}
      <button
        role="tab"
        aria-selected={view.mode === 'design'}
        data-testid="mode-design"
        disabled={locked}
        title={locked ? t('mode-locked') : undefined}
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
  {#if agents}
    <span class="auto-chip" data-testid="agents-chip" title={automation.status.agents.join(', ')}
      ><Icon name="sparkle" /><span>{t('auto-agents', { n: agents })}</span></span
    >
  {/if}
  {#if script}
    <span class="auto-chip" data-testid="script-chip"
      ><Icon name="clock" /><span>{t('auto-script', { name: script.name })}</span><button
        class="icon-btn"
        title={t('auto-cancel')}
        aria-label={t('auto-cancel')}
        data-testid="script-cancel"
        onclick={() => run(() => service.cancelScripts())}><Icon name="close" /></button
      ></span
    >
  {/if}
  <StateChip />
  <button
    class="btn"
    id="btnConnectTop"
    data-testid="connect-top"
    onclick={() =>
      run(() =>
        connected
          ? service.dispatch('machine.disconnect', {})
          : service.dispatch('view.open_panel', { name: 'machine' }),
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
    onclick={() => run(() => service.dispatch('view.open_dialog', { name: 'settings' }))}
  >
    <Icon name="gear" />
  </button>
  <EStop />
</header>

<style>
  .auto-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--fk-space-1);
    padding: 0 var(--fk-space-2);
    border-radius: var(--fk-radius-md);
    background: color-mix(in srgb, var(--fk-color-accent) 18%, transparent);
    font-size: var(--fk-font-size-sm);
    white-space: nowrap;
  }
</style>
