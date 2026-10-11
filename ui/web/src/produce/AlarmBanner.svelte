<script lang="ts">
  // Over the canvas when a job was interrupted or the board is in alarm: what
  // happened, and Unlock / Recover job… (design/ux.md "Segurança" 6).
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import { causeId } from './job';

  const { machine, service, run } = app();
  const resume = $derived(machine.state?.resume ?? null);
  const alarm = $derived(machine.visual === 'alarm');
  const show = $derived(alarm || (resume !== null && machine.visual !== 'run'));
</script>

{#if show}
  <div class="banner" role="alert" data-testid="alarm-banner">
    <span class="banner-ico"><Icon name="warn" /></span>
    <div class="banner-text">
      <strong>{alarm ? t('state-alarm') : t('recover-title')}</strong>
      {#if resume}<span
          >{t(causeId(resume.issue))} · {t('machine-line', { a: resume.executed, b: resume.target })}</span
        >{/if}
    </div>
    <div class="banner-actions">
      {#if alarm}
        <button
          class="btn"
          data-testid="banner-unlock"
          onclick={() => run(() => service.dispatch('machine.unlock', {}))}
        >
          <Icon name="unlock" /><span>{t('machine-unlock')}</span>
        </button>
      {/if}
      {#if resume}
        <button
          class="btn primary"
          data-testid="banner-recover"
          onclick={() => run(() => service.dispatch('view.open_dialog', { dialog: 'recover' }))}
        >
          <Icon name="repeat" /><span>{t('recover-open')}</span>
        </button>
      {/if}
    </div>
  </div>
{/if}
