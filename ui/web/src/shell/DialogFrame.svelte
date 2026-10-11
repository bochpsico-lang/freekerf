<script lang="ts">
  // A dialog of the design system: not modal (STOP stays reachable above the
  // backdrop), header with a close button, scrollable body, actions in the footer.
  // Focus goes to the first control when it opens.
  import type { Snippet } from 'svelte';

  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';

  let {
    title,
    testid,
    wide = false,
    closable = true,
    onclose,
    children,
    footer,
  }: {
    title: string;
    testid: string;
    wide?: boolean;
    closable?: boolean;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  let el: HTMLDivElement;
  $effect(() => {
    const first = el.querySelector<HTMLElement>(
      '.dlg-body button, .dlg-body input, .dlg-body select, footer button, header button',
    );
    first?.focus();
  });
</script>

<div
  class="dlg"
  class:welcome={wide}
  role="dialog"
  aria-modal="false"
  aria-label={title}
  data-testid={testid}
  bind:this={el}
>
  <header>
    <h2>{title}</h2>
    {#if closable}
      <button class="icon-btn" title={t('common-close')} aria-label={t('common-close')} onclick={onclose}>
        <Icon name="close" />
      </button>
    {/if}
  </header>
  <div class="dlg-body">{@render children()}</div>
  {#if footer}<footer>{@render footer()}</footer>{/if}
</div>
