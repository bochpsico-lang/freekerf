<script lang="ts">
  // "Ready to burn?" (design/ux.md "Segurança" 3): what will run, the bench
  // checklist (a reminder, not a lock), and "Fire laser" enabled only after the
  // countdown. The app checks the countdown again when the button is pressed
  // (`confirm_fire`), so a hurried click or a script cannot skip it.
  import { onDestroy } from 'svelte';

  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import { formatDuration } from '../lib/format';
  import DialogFrame from '../shell/DialogFrame.svelte';

  const { view, service, run } = app();
  const request = $derived(view.state?.fire_request ?? null);

  let left = $state(0);
  let timer: ReturnType<typeof setInterval> | null = null;
  let shownId = -1;

  $effect(() => {
    const r = request;
    if (!r || r.id === shownId) return;
    shownId = r.id;
    left = Math.ceil(r.countdown_ms / 1000);
    if (timer) clearInterval(timer);
    timer = setInterval(() => {
      left = Math.max(0, left - 1);
      if (left === 0 && timer) {
        clearInterval(timer);
        timer = null;
      }
    }, 1000);
  });
  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  const summary = $derived.by(() => {
    const s = request?.summary;
    if (!s) return '';
    const parts: string[] = [];
    if (s.program) parts.push(`${s.program} · ${s.lines ?? 0}`);
    if (s.estimated_ms) parts.push(`≈ ${formatDuration(s.estimated_ms)}`);
    if (s.bounds)
      parts.push(`${(s.bounds[2] - s.bounds[0]).toFixed(1)} × ${(s.bounds[3] - s.bounds[1]).toFixed(1)} mm`);
    if (s.code) parts.push(s.code);
    return parts.join(' · ');
  });

  const cancel = () => run(() => service.dispatch('fire.cancel', { id: request?.id ?? null }));
  const fire = () => {
    const r = request;
    if (r && left === 0) void run(() => service.confirmFire(r.id));
  };
</script>

<DialogFrame title={t('preflight-title')} testid="dialog-fire_confirmation" closable={false} onclose={cancel}>
  <p class="pre-sum mono" data-testid="fire-summary">{summary || request?.action || '—'}</p>
  <ul class="checklist">
    {#each ['preflight-c1', 'preflight-c2', 'preflight-c3', 'preflight-c4'] as c (c)}
      <li><label class="check"><input type="checkbox" /><span>{t(c)}</span></label></li>
    {/each}
  </ul>
  <p class="hint">{t('preflight-note')}</p>
  {#snippet footer()}
    <button class="btn" data-testid="fire-cancel" onclick={cancel}>{t('preflight-cancel')}</button>
    <button
      class="btn primary big"
      id="btnFire"
      data-testid="fire"
      disabled={!request || left > 0}
      onclick={fire}
    >
      <Icon name="laser" /><span>{left > 0 ? t('preflight-wait', { s: left }) : t('preflight-fire')}</span>
    </button>
  {/snippet}
</DialogFrame>
