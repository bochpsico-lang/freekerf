<script lang="ts">
  // Editing tools (Design mode). Select, rectangle, ellipse and text are the view
  // state's tool (`view.set_tool`); import opens its dialog; undo and redo are
  // document actions. Node editing, tracing, boolean operations, offset, array and
  // the material test arrive with M2 (listed, disabled).
  import { app } from '../context';
  import type { Tool } from '../gen/api';
  import type { IconName } from '../gen/icons';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';

  const { view, doc, service, run } = app();

  const tools: ([IconName, string, Tool | 'import' | null] | null)[] = [
    ['select', 'tool-select', 'select'],
    ['node', 'tool-node', null],
    ['rect', 'tool-rect', 'rect'],
    ['ellipse', 'tool-ellipse', 'ellipse'],
    ['text', 'tool-text', 'text'],
    null,
    ['import', 'tool-import', 'import'],
    ['trace', 'tool-trace', null],
    null,
    ['weld', 'tool-weld', null],
    ['subtract', 'tool-subtract', null],
    ['offset', 'tool-offset', null],
    ['array', 'tool-array', null],
    ['test', 'tool-test', null],
  ];

  function pick(target: Tool | 'import') {
    return run(() =>
      target === 'import'
        ? service.dispatch('view.open_dialog', { name: 'import' })
        : service.dispatch('view.set_tool', { tool: target }),
    );
  }
</script>

<nav class="toolrail" aria-label="Tools">
  {#each tools as tool, i (i)}
    {#if tool}
      {@const [icon, label, target] = tool}
      <button
        class="tool"
        data-tool={icon}
        disabled={!target || !doc}
        aria-pressed={target && target !== 'import' ? view.tool === target : undefined}
        title={t(label)}
        aria-label={t(label)}
        onclick={() => target && pick(target)}><Icon name={icon} /></button
      >
    {:else}
      <hr />
    {/if}
  {/each}
  <div class="rail-spacer"></div>
  <button
    class="tool"
    data-tool="undo"
    disabled={!doc?.summary?.can_undo}
    title={t('tool-undo')}
    aria-label={t('tool-undo')}
    onclick={() => doc && run(() => doc.service.dispatch('document.undo', {}))}><Icon name="undo" /></button
  >
  <button
    class="tool"
    data-tool="redo"
    disabled={!doc?.summary?.can_redo}
    title={t('tool-redo')}
    aria-label={t('tool-redo')}
    onclick={() => doc && run(() => doc.service.dispatch('document.redo', {}))}><Icon name="redo" /></button
  >
</nav>
