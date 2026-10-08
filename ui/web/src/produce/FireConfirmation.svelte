<script lang="ts">
  // "Ready to burn?" (design/ux.md "Segurança" 3). This PR shows what was asked;
  // the confirmation itself (checklist, countdown, Rust-side gate) comes with the
  // fire confirmation PR.
  import type { LaserRequest } from '../context';
  import { t } from '../i18n/index.svelte';
  import DialogFrame from '../shell/DialogFrame.svelte';

  let { request, onclose }: { request: LaserRequest | null; onclose: () => void } = $props();
</script>

<DialogFrame title={t('preflight-title')} testid="dialog-fire_confirmation" {onclose}>
  <p class="pre-sum mono">{request ? request.action : '—'}</p>
  <p class="hint">{t('common-soon')}</p>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t('preflight-cancel')}</button>
  {/snippet}
</DialogFrame>
