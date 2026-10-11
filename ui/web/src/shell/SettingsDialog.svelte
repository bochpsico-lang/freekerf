<script lang="ts">
  // Settings (PLAN §14: the UI never reads settings.toml; it reads `settings.get`
  // and writes with `settings.set`). Safety and automation are user-only: shown
  // here read-only (`freekerf config set` changes them; editable in the app in M4b).
  import { app } from '../context';
  import type { Settings } from '../gen/api';
  import { completeness, LOCALES, localeName, t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import DialogFrame from './DialogFrame.svelte';

  let { onclose }: { onclose: () => void } = $props();
  const { settings, service, run } = app();
  type Tab = 'display' | 'machine' | 'job' | 'safety' | 'automation';
  const tabs: Tab[] = ['display', 'machine', 'job', 'safety', 'automation'];
  let tab: Tab = $state('display');

  const set = (key: string, value: unknown) => run(() => service.dispatch('settings.set', { key, value }));
  const ui = $derived(settings.ui);
  const s = $derived<Settings>(settings.settings ?? {});
  const m = $derived(s.machine ?? {});
  const num = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);
</script>

<DialogFrame title={t('settings-title')} testid="dialog-settings" {onclose}>
  <div class="seg wrap settings-tabs" role="tablist" aria-label={t('settings-title')}>
    {#each tabs as v (v)}
      <button
        role="tab"
        aria-selected={tab === v}
        class:on={tab === v}
        data-testid="settings-tab-{v}"
        onclick={() => (tab = v)}>{t(`settings-tab-${v}`)}</button
      >
    {/each}
  </div>

  {#if tab === 'display'}
    <div class="form-grid">
      <span>{t('settings-lang')}</span>
      <select
        data-testid="language"
        value={ui.language}
        aria-label={t('settings-lang')}
        onchange={(e) => set('ui.language', e.currentTarget.value)}
      >
        <option value="auto">{t('settings-langAuto')}</option>
        {#each LOCALES as l (l)}
          <option value={l}
            >{localeName(l)}{completeness(l) < 1 ? ` (${Math.round(completeness(l) * 100)}%)` : ''}</option
          >
        {/each}
      </select>
      <span>{t('settings-theme')}</span>
      <div class="seg" role="radiogroup" aria-label={t('settings-theme')}>
        {#each [['dark', 'moon'], ['light', 'sun']] as const as [v, icon] (v)}
          <button
            role="radio"
            aria-checked={ui.theme === v}
            class:on={ui.theme === v}
            data-testid="theme-{v}"
            onclick={() => set('ui.theme', v)}><Icon name={icon} /><span>{t(`theme-${v}`)}</span></button
          >
        {/each}
      </div>
      <span>{t('settings-contrast')}</span>
      <label class="switch"
        ><input
          type="checkbox"
          aria-label={t('settings-contrast')}
          data-testid="high-contrast"
          checked={ui.high_contrast}
          onchange={(e) => set('ui.high_contrast', e.currentTarget.checked)}
        /><span></span></label
      >
      <span>{t('settings-density')}</span>
      <div class="seg wrap" role="radiogroup" aria-label={t('settings-density')}>
        {#each ['auto', 'compact', 'comfortable'] as const as v (v)}
          <button
            role="radio"
            aria-checked={ui.density === v}
            class:on={ui.density === v}
            data-testid="density-{v}"
            onclick={() => set('ui.density', v)}><span>{t(`density-${v}`)}</span></button
          >
        {/each}
      </div>
      <span>{t('settings-units')}</span>
      <div class="seg" role="radiogroup" aria-label={t('settings-units')}>
        {#each ['mm', 'in'] as const as v (v)}
          <button
            role="radio"
            aria-checked={ui.units === v}
            class:on={ui.units === v}
            onclick={() => set('ui.units', v)}>{t(`units-${v}`)}</button
          >
        {/each}
      </div>
    </div>
  {:else if tab === 'machine'}
    <div class="form-grid">
      <label for="s-fw">{t('settings-firmware')}</label>
      <select
        id="s-fw"
        value={m.firmware ?? 'Grbl'}
        onchange={(e) => set('machine.firmware', e.currentTarget.value)}
      >
        {#each ['Grbl', 'Smoothie', 'Marlin', 'VigoWork'] as v (v)}<option value={v}>{v}</option>{/each}
      </select>
      <label for="s-baud">{t('settings-baud')}</label>
      <select id="s-baud" value={String(m.baud ?? 115200)} onchange={(e) => set('machine.baud', num(e))}>
        {#each [9600, 19200, 38400, 57600, 115200, 230400, 250000, 460800, 921600] as v (v)}<option
            value={String(v)}>{v}</option
          >{/each}
      </select>
      <label for="s-stream">{t('settings-streaming')}</label>
      <select
        id="s-stream"
        value={m.streaming_mode ?? 'Buffered'}
        onchange={(e) => set('machine.streaming_mode', e.currentTarget.value)}
      >
        {#each ['Buffered', 'Synchronous', 'RepeatOnError'] as v (v)}<option value={v}
            >{t(`settings-streaming-${v}`)}</option
          >{/each}
      </select>
      <label for="s-poll">{t('settings-statusMs')}</label>
      <input
        id="s-poll"
        type="number"
        min="50"
        step="50"
        value={m.status_query_ms ?? 200}
        onchange={(e) => set('machine.status_query_ms', num(e))}
      />
      <span>{t('settings-resetOnConnect')}</span>
      <label class="switch"
        ><input
          type="checkbox"
          aria-label={t('settings-resetOnConnect')}
          checked={m.reset_on_connect ?? true}
          onchange={(e) => set('machine.reset_on_connect', e.currentTarget.checked)}
        /><span></span></label
      >
      <span>{t('settings-supportPwm')}</span>
      <label class="switch"
        ><input
          type="checkbox"
          aria-label={t('settings-supportPwm')}
          checked={m.support_pwm ?? true}
          onchange={(e) => set('machine.support_pwm', e.currentTarget.checked)}
        /><span></span></label
      >
      <span>{t('settings-autoCooling')}</span>
      <label class="switch"
        ><input
          type="checkbox"
          aria-label={t('settings-autoCooling')}
          checked={m.auto_cooling?.enabled ?? false}
          onchange={(e) => set('machine.auto_cooling.enabled', e.currentTarget.checked)}
        /><span></span></label
      >
      <label for="s-jogspeed">{t('settings-jogSpeed')}</label>
      <input
        id="s-jogspeed"
        type="number"
        min="1"
        value={s.jog?.speed_mm_min ?? 1000}
        onchange={(e) => set('jog.speed_mm_min', num(e))}
      />
    </div>
  {:else if tab === 'job'}
    {#each [['job.header', 'settings-header', s.job?.header], ['job.passes', 'settings-passes', s.job?.passes], ['job.footer', 'settings-footer', s.job?.footer]] as [key, label, value] (key)}
      <div class="field">
        <label for={key}>{t(label ?? '')}</label>
        <textarea
          id={key}
          class="textarea mono"
          spellcheck="false"
          value={value ?? ''}
          onchange={(e) => set(key ?? '', e.currentTarget.value)}></textarea>
      </div>
    {/each}
    <p class="hint">{t('settings-jobHint')}</p>
  {:else if tab === 'safety'}
    <div class="form-grid" data-testid="settings-safety">
      <span>{t('settings-countdown')}</span>
      <b class="mono">{s.safety?.fire_countdown_s ?? 3} s</b>
    </div>
    <p class="hint">{t('settings-userOnly')}</p>
  {:else}
    <div class="form-grid" data-testid="settings-automation">
      <span>{t('settings-mcp')}</span>
      <b>{t(s.automation?.mcp?.enabled ? 'settings-on' : 'settings-off')}</b>
      <span>{t('settings-mcpMachine')}</span>
      <b>{t(s.automation?.mcp?.allow_machine ? 'settings-on' : 'settings-off')}</b>
    </div>
    <p class="hint">{t('settings-automationNote')}</p>
    <p class="hint">{t('settings-userOnly')}</p>
  {/if}

  {#snippet footer()}
    <button class="btn" onclick={() => run(() => service.dispatch('view.open_dialog', { name: 'welcome' }))}>
      <Icon name="sparkle" /><span>{t('settings-welcome')}</span>
    </button>
    <button
      class="btn"
      onclick={() => run(() => service.dispatch('view.open_dialog', { name: 'shortcuts' }))}
    >
      <Icon name="info" /><span>{t('settings-shortcuts')}</span>
    </button>
  {/snippet}
</DialogFrame>

<style>
  .settings-tabs {
    align-self: flex-start;
  }
</style>
