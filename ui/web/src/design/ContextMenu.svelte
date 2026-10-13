<script lang="ts">
  // The canvas context menu of the Design mode (right click, long press): copy,
  // cut, paste (where it was opened), duplicate, delete. Keys: ↑ ↓ Home End,
  // Enter or Space to choose, Esc or Tab to close; a press outside closes it.
  import { onMount } from 'svelte';

  import { t } from '../i18n/index.svelte';
  import type { EditCommand } from './editor';

  let {
    x,
    y,
    selected,
    canPaste,
    onpick,
    onclose,
  }: {
    /** Where it opened, CSS pixels in the stage. */
    x: number;
    y: number;
    /** Objects are selected (copy, cut, duplicate, delete). */
    selected: boolean;
    /** The clipboard holds objects. */
    canPaste: boolean;
    onpick: (cmd: EditCommand) => void;
    onclose: () => void;
  } = $props();

  const items: [EditCommand, string, string][] = [
    ['copy', 'kb-copy', 'Ctrl+C'],
    ['cut', 'kb-cut', 'Ctrl+X'],
    ['paste', 'kb-paste', 'Ctrl+V'],
    ['duplicate', 'kb-dup', 'Ctrl+D'],
    ['combine', 'kb-combine', 'Ctrl+G'],
    ['split', 'kb-split', 'Ctrl+Shift+G'],
    ['delete', 'kb-del', 'Del'],
  ];
  const enabled = (c: EditCommand) => (c === 'paste' ? canPaste : selected);

  let menu: HTMLElement | undefined = $state();
  // kept inside the stage once its size is known
  let pos: { left: number; top: number } | null = $state(null);

  const buttons = () => [...(menu?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];

  onMount(() => {
    const parent = menu?.offsetParent?.getBoundingClientRect();
    const box = menu?.getBoundingClientRect();
    pos = {
      left: parent && box ? Math.max(0, Math.min(x, parent.width - box.width)) : x,
      top: parent && box ? Math.max(0, Math.min(y, parent.height - box.height)) : y,
    };
    buttons()[0]?.focus();
    const outside = (e: PointerEvent) => {
      if (!menu?.contains(e.target as Node)) onclose();
    };
    window.addEventListener('pointerdown', outside, true);
    window.addEventListener('blur', onclose);
    return () => {
      window.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('blur', onclose);
    };
  });

  function keydown(e: KeyboardEvent) {
    const list = buttons();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    const go = (n: number) => list[(n + list.length) % list.length]?.focus();
    switch (e.key) {
      case 'Escape':
      case 'Tab':
        onclose();
        break;
      case 'ArrowDown':
        go(i + 1);
        break;
      case 'ArrowUp':
        go(i < 0 ? -1 : i - 1);
        break;
      case 'Home':
        go(0);
        break;
      case 'End':
        go(-1);
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<div
  class="context-menu"
  role="menu"
  tabindex="-1"
  data-testid="context-menu"
  bind:this={menu}
  style:left={`${pos?.left ?? x}px`}
  style:top={`${pos?.top ?? y}px`}
  onkeydown={keydown}
  oncontextmenu={(e) => e.preventDefault()}
>
  {#each items as [cmd, label, keys] (cmd)}
    <button
      role="menuitem"
      data-cmd={cmd}
      disabled={!enabled(cmd)}
      onclick={() => {
        // pick first: closing drops the place where the menu opened
        onpick(cmd);
        onclose();
      }}
    >
      <span>{t(label)}</span><kbd>{keys}</kbd>
    </button>
  {/each}
</div>
