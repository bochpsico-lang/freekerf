<script lang="ts">
  // Editing tools (Design mode). The tools arrive with the Design mode screens
  // (M4a PR 8, on the UI contract); here they are listed and disabled.
  import type { IconName } from '../gen/icons';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';

  const tools: ([IconName, string] | null)[] = [
    ['select', 'tool-select'],
    ['node', 'tool-node'],
    ['rect', 'tool-rect'],
    ['ellipse', 'tool-ellipse'],
    ['text', 'tool-text'],
    null,
    ['import', 'tool-import'],
    ['trace', 'tool-trace'],
    null,
    ['weld', 'tool-weld'],
    ['subtract', 'tool-subtract'],
    ['offset', 'tool-offset'],
    ['array', 'tool-array'],
    ['test', 'tool-test'],
  ];
</script>

<nav class="toolrail" aria-label="Tools">
  {#each tools as tool, i (i)}
    {#if tool}
      <button class="tool" disabled title={t(tool[1])} aria-label={t(tool[1])}><Icon name={tool[0]} /></button
      >
    {:else}
      <hr />
    {/if}
  {/each}
  <div class="rail-spacer"></div>
  <button class="tool" disabled title={t('tool-undo')} aria-label={t('tool-undo')}
    ><Icon name="undo" /></button
  >
  <button class="tool" disabled title={t('tool-redo')} aria-label={t('tool-redo')}
    ><Icon name="redo" /></button
  >
</nav>
