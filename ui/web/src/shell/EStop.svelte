<script lang="ts">
  // STOP (design/ux.md "Segurança" 1): soft reset, laser off. Always visible and
  // clickable, above dialogs and drawers.
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';

  const { service } = app();
</script>

<button
  class="estop"
  data-testid="estop"
  title={t('estop-hint')}
  aria-label={t('estop')}
  onclick={() => void service.dispatch('machine.stop', {})}
>
  <svg viewBox="0 0 24 24" aria-hidden="true"
    ><path d="M8.3 2.5h7.4l5.8 5.8v7.4l-5.8 5.8H8.3l-5.8-5.8V8.3z" /></svg
  >
  <span>{t('estop')}</span>
</button>
