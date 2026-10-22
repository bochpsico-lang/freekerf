<script lang="ts">
  // Under the port picker when this user may not open the port (Linux: not in the
  // group that owns the device): what to do, and the command to copy
  // (doc/features/connection.md "Permissions on Linux").
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';

  /** The port, and the group that owns it (`dialout` when unknown). */
  const { port, group }: { port: string; group?: string | null } = $props();

  const name = $derived(group || 'dialout');
  const command = $derived(`sudo usermod -aG ${name} $USER`);
  let copied = $state(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      copied = true;
    } catch {
      copied = false; // the command stays on screen to copy by hand
    }
  }
</script>

<div class="port-denied" role="alert" data-testid="port-denied">
  <span class="ico"><Icon name="warn" /></span>
  <div class="body">
    <p>{t('machine-portDenied', { port, group: name })}</p>
    <div class="cmd">
      <code data-testid="port-denied-command">{command}</code>
      <button
        class="icon-btn"
        title={t(copied ? 'machine-portDeniedCopied' : 'machine-portDeniedCopy')}
        aria-label={t(copied ? 'machine-portDeniedCopied' : 'machine-portDeniedCopy')}
        data-testid="port-denied-copy"
        onclick={copy}
      >
        <Icon name={copied ? 'check' : 'copy'} />
      </button>
    </div>
  </div>
</div>

<style>
  .port-denied {
    display: flex;
    gap: var(--fk-space-2);
    padding: var(--fk-space-2);
    border: 1px solid var(--fk-color-warning);
    border-radius: var(--fk-radius-md);
    font-size: var(--fk-font-size-sm);
  }
  .ico {
    color: var(--fk-color-warning);
    flex: none;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--fk-space-1);
    min-width: 0;
  }
  p {
    margin: 0;
  }
  .cmd {
    display: flex;
    align-items: center;
    gap: var(--fk-space-1);
  }
  code {
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    white-space: nowrap;
    font-family: var(--fk-font-family-mono);
    user-select: all;
  }
</style>
