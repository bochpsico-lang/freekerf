<script lang="ts">
  // Display and input settings (ui.* in settings.toml, through settings.set).
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import DialogFrame from './DialogFrame.svelte';

  let { onclose }: { onclose: () => void } = $props();
  const { settings, service, run } = app();
  const set = (key: string, value: unknown) => run(() => service.dispatch('settings.set', { key, value }));
  const ui = $derived(settings.ui);
</script>

<DialogFrame title={t('settings-title')} testid="dialog-settings" {onclose}>
  <div class="form-grid">
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
  {#snippet footer()}
    <button
      class="btn"
      onclick={() => run(() => service.dispatch('view.open_dialog', { dialog: 'shortcuts' }))}
    >
      <Icon name="info" /><span>{t('settings-shortcuts')}</span>
    </button>
  {/snippet}
</DialogFrame>
