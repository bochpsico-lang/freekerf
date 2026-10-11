<script lang="ts">
  import { t } from '../i18n/index.svelte';
  import DialogFrame from './DialogFrame.svelte';
  import { SHORTCUT_TABLE } from './shortcuts';

  let { onclose }: { onclose: () => void } = $props();
  const groups = [...new Set(SHORTCUT_TABLE.map(([g]) => g))];
</script>

<DialogFrame title={t('shortcuts-title')} testid="dialog-shortcuts" {onclose}>
  <table class="kbd-table">
    <tbody>
      {#each groups as g (g)}
        <tr><th colspan="2">{t(g)}</th></tr>
        {#each SHORTCUT_TABLE.filter(([x]) => x === g) as [, keys, desc] (desc)}
          <tr><td><kbd>{keys}</kbd></td><td>{t(desc)}</td></tr>
        {/each}
      {/each}
    </tbody>
  </table>
</DialogFrame>
