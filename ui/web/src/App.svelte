<script lang="ts">
  import type { AppService } from './service';
  import type { MachineState } from './state/machine.svelte';

  let { service, machine }: { service: AppService; machine: MachineState } = $props();

  const labels = {
    offline: 'Disconnected',
    connecting: 'Connecting…',
    idle: 'Ready',
    run: 'Running',
    hold: 'Paused',
    alarm: 'Alarm',
  } as const;

  let error = $state('');

  async function run(action: () => Promise<unknown>): Promise<void> {
    error = '';
    try {
      await action();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<header class="topbar">
  <span class="brand">Free<b>Kerf</b></span>
  <span class="spacer"></span>
  <span class="state-chip" data-state={machine.visual} role="status">{labels[machine.visual]}</span>
  {#if machine.visual === 'offline'}
    <button class="btn" onclick={() => run(() => service.dispatch('machine.connect', { emulator: true }))}>
      Connect to the emulator
    </button>
  {:else}
    <button class="btn" onclick={() => run(() => service.dispatch('machine.disconnect', {}))}
      >Disconnect</button
    >
  {/if}
  <button class="estop" onclick={() => run(() => service.dispatch('machine.stop', {}))}>STOP</button>
</header>
<main class="stage">
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if machine.state}
    <p class="mono">
      X {machine.state.machine.work_position.x.toFixed(2)} · Y {machine.state.machine.work_position.y.toFixed(
        2,
      )}
    </p>
  {/if}
</main>
