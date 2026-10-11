<script lang="ts">
  // Editing tools (Design mode). Select, rectangle, ellipse and text are the view
  // state's tool (`view.set_tool`); import opens its dialog; weld and subtract
  // combine the selection (`document.boolean`); offset and array open the inspector,
  // where their parameters are; undo and redo are document actions. Node editing,
  // tracing and the material test come later (listed, disabled).
  import { app } from '../context';
  import type { Tool } from '../gen/api';
  import type { IconName } from '../gen/icons';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';

  const { view, doc, service, run } = app();

  type Target = Tool | 'import' | 'union' | 'difference' | 'inspector';
  const tools: ([IconName, string, Target | null] | null)[] = [
    ['select', 'tool-select', 'select'],
    ['node', 'tool-node', null],
    ['rect', 'tool-rect', 'rect'],
    ['ellipse', 'tool-ellipse', 'ellipse'],
    ['text', 'tool-text', 'text'],
    null,
    ['import', 'tool-import', 'import'],
    ['trace', 'tool-trace', null],
    null,
    ['weld', 'tool-weld', 'union'],
    ['subtract', 'tool-subtract', 'difference'],
    ['offset', 'tool-offset', 'inspector'],
    ['array', 'tool-array', 'inspector'],
    ['test', 'tool-test', null],
  ];

  const combining = (t: Target) => t === 'union' || t === 'difference';

  function pick(target: Target) {
    return run(() => {
      if (target === 'import') return service.dispatch('view.open_dialog', { name: 'import' });
      if (target === 'inspector') return service.dispatch('view.open_panel', { name: 'object' });
      if (target === 'union' || target === 'difference')
        return service.dispatch('document.boolean', { op: target, ids: doc.selection });
      return service.dispatch('view.set_tool', { tool: target });
    });
  }

  /** Whether a tool can be used now. */
  function usable(target: Target | null): boolean {
    if (!target) return false;
    if (combining(target)) return doc.selection.length >= 2;
    if (target === 'inspector') return doc.selection.length >= 1;
    return true;
  }
</script>

<nav class="toolrail" aria-label="Tools">
  {#each tools as tool, i (i)}
    {#if tool}
      {@const [icon, label, target] = tool}
      <button
        class="tool"
        data-tool={icon}
        disabled={!usable(target)}
        aria-pressed={target && !['import', 'inspector', 'union', 'difference'].includes(target)
          ? view.tool === target
          : undefined}
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
    disabled={!doc.summary?.can_undo}
    title={t('tool-undo')}
    aria-label={t('tool-undo')}
    onclick={() => run(() => doc.service.dispatch('history.undo', {}))}><Icon name="undo" /></button
  >
  <button
    class="tool"
    data-tool="redo"
    disabled={!doc.summary?.can_redo}
    title={t('tool-redo')}
    aria-label={t('tool-redo')}
    onclick={() => run(() => doc.service.dispatch('history.redo', {}))}><Icon name="redo" /></button
  >
</nav>
