<script lang="ts">
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import { formatLength } from '../lib/format';

  let { zoom }: { zoom: number } = $props();
  const { machine, settings, env, service, run } = app();
  const units = $derived(settings.ui.units);
  const head = $derived(machine.state?.machine.work_position);
  const progress = $derived.by(() => {
    const m = machine.state?.machine;
    return m && m.in_program && m.job_target > 0 ? Math.floor((100 * m.job_executed) / m.job_target) : null;
  });
</script>

<footer class="statusbar" data-testid="statusbar">
  <span class="sb-state">{t(`state-${machine.visual}`)}</span>
  <span class="sb-item"
    >{t('status-head')}
    <b class="mono">{head ? `${formatLength(head.x, units)}, ${formatLength(head.y, units)}` : '—'}</b></span
  >
  {#if progress !== null}<span class="sb-item sb-progress"><b class="mono">{progress}%</b></span>{/if}
  <span class="sb-spacer"></span>
  <button
    class="sb-btn"
    title={t('settings-units')}
    onclick={() =>
      run(() => service.dispatch('settings.set', { key: 'ui.units', value: units === 'mm' ? 'in' : 'mm' }))}
    >{t(`units-${units}`)}</button
  >
  <span class="sb-item hide-narrow">{t('status-zoom')} <b class="mono">{Math.round(zoom * 100)}%</b></span>
  <span class="sb-item hide-narrow sb-input"
    ><Icon name={env.input === 'touch' ? 'touch' : 'mouse'} />{settings.ui.density === 'auto'
      ? t('sb-auto')
      : ''}</span
  >
</footer>
