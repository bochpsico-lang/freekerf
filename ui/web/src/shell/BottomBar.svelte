<script lang="ts">
  // Narrow layout: Start, Pause and Frame in the thumb zone, with STOP (design/ux.md).
  import type { Snippet } from 'svelte';

  import EStop from './EStop.svelte';

  let { children }: { children?: Snippet } = $props();
</script>

<div class="bottombar" data-testid="bottombar">
  {@render children?.()}
  <EStop />
</div>
