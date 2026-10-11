<script lang="ts">
  // Every open dialog of the view state, bottom to top, over a backdrop that never
  // covers the top bar (STOP and the machine state stay visible and clickable).
  import type { Snippet } from 'svelte';

  import { app } from '../context';
  import type { Dialog } from '../gen/api';
  import { t } from '../i18n/index.svelte';
  import DialogFrame from './DialogFrame.svelte';
  import SettingsDialog from './SettingsDialog.svelte';
  import ShortcutsDialog from './ShortcutsDialog.svelte';

  /** Dialogs provided by other parts of the app (Produce mode, fire confirmation...). */
  let { extra = {} }: { extra?: Partial<Record<Dialog, Snippet<[() => void]>>> } = $props();
  const { view, service, run } = app();
  const close = (dialog: Dialog) => () => run(() => service.dispatch('view.close_dialog', { name: dialog }));
</script>

{#if view.dialogs.length}
  <!-- the backdrop closes the top dialog, except the fire confirmation (no accidental cancel) -->
  <div
    class="dlg-backdrop"
    role="presentation"
    data-testid="dialog-backdrop"
    onclick={() => {
      if (view.topDialog && view.topDialog !== 'fire_confirmation') void close(view.topDialog)();
    }}
  ></div>
  {#each view.dialogs as d (d)}
    {#if extra[d]}
      {@render extra[d](close(d))}
    {:else if d === 'settings'}
      <SettingsDialog onclose={close(d)} />
    {:else if d === 'shortcuts'}
      <ShortcutsDialog onclose={close(d)} />
    {:else}
      <DialogFrame title={d} testid="dialog-{d}" onclose={close(d)}>
        <p class="hint">{t('common-soon')}</p>
      </DialogFrame>
    {/if}
  {/each}
{/if}
