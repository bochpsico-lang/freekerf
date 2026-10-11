<script lang="ts">
  // An icon of the design system (ui/web/src/gen/icons.ts, original FreeKerf icons).
  import { ICONS, type IconName } from '../gen/icons';

  let { name, class: cls = '' }: { name: IconName; class?: string } = $props();
</script>

<svg class="ico {cls}" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
  <!-- eslint-disable-next-line svelte/no-at-html-tags -- generated, trusted SVG bodies -->
  {@html ICONS[name] ?? ICONS.dot}
</svg>
