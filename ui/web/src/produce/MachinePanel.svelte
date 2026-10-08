<script lang="ts">
  // Machine panel (Produce mode). This PR: connection and position; the job, jog,
  // origin, frame and custom buttons come with the Produce mode PR.
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import { formatLength } from '../lib/format';

  const { machine, settings, service, run } = app();
  const connected = $derived(machine.visual !== 'offline');
  const pos = $derived(machine.state?.machine.work_position);
</script>

<div class="machine" class:offline={!connected}>
  <div class="group conn">
    <button
      class="btn wide"
      data-testid="connect-emulator"
      onclick={() =>
        run(() =>
          connected
            ? service.dispatch('machine.disconnect', {})
            : service.dispatch('machine.connect', { emulator: true }),
        )}
    >
      <Icon name={connected ? 'plugOff' : 'plug'} /><span
        >{t(connected ? 'machine-disconnect' : 'machine-connect')}</span
      >
    </button>
  </div>
  <div class="group jog">
    <div class="dro" aria-live="off">
      <div><span>X</span><b data-testid="dro-x">{pos ? formatLength(pos.x, settings.ui.units) : '—'}</b></div>
      <div><span>Y</span><b data-testid="dro-y">{pos ? formatLength(pos.y, settings.ui.units) : '—'}</b></div>
    </div>
    {#if !connected}<p class="hint">{t('machine-notConnected')}</p>{/if}
  </div>
</div>
