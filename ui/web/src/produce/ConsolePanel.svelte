<script lang="ts">
  // G-code console: sent lines, answers and messages with a prefix per kind (colour
  // is never the only cue). `$` commands go straight to the machine; any other
  // line can fire the laser and goes through the fire confirmation.
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import { consoleLine } from '../state/console.svelte';

  const { console: con, machine, service, run, laser } = app();
  let line = $state('');
  let log: HTMLDivElement;
  const connected = $derived(machine.visual !== 'offline');
  const rows = $derived(con.rows.map(consoleLine));

  $effect(() => {
    void rows.length;
    if (log) log.scrollTop = log.scrollHeight;
  });

  async function send(e: SubmitEvent) {
    e.preventDefault();
    const text = line.trim();
    if (!text) return;
    line = '';
    if (text.startsWith('$')) await run(() => service.dispatch('machine.system_command', { line: text }));
    else await laser({ action: 'machine.send', params: { line: text } });
  }
</script>

<div class="console" role="log" aria-live="polite" bind:this={log} data-testid="console">
  {#each rows as r, i (i)}
    <div class="ln {r.kind}">{r.text}</div>
  {/each}
</div>
<form class="console-in" onsubmit={send}>
  <input
    class="mono"
    autocomplete="off"
    spellcheck="false"
    placeholder={t('console-placeholder')}
    aria-label={t('console-placeholder')}
    disabled={!connected}
    data-testid="console-input"
    bind:value={line}
  />
  <button class="btn" disabled={!connected}><Icon name="right" /><span>{t('console-send')}</span></button>
</form>
