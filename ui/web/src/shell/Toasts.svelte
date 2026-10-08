<script lang="ts">
  // Toasts of the view state; each dismisses itself after 3.5 s (errors 6 s) through
  // `view.dismiss_toast`.
  import { app } from '../context';
  import type { Toast } from '../gen/api';
  import type { IconName } from '../gen/icons';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';

  const { view, service } = app();
  const icons: Record<Toast['level'], IconName> = {
    info: 'info',
    ok: 'check',
    warning: 'warn',
    error: 'warn',
  };
  const cls: Record<Toast['level'], string> = { info: '', ok: 'ok', warning: 'warn', error: 'err' };
  // ids already scheduled (not reactive on purpose)
  const scheduled: Record<number, true> = {};

  $effect(() => {
    const toasts = view.state?.toasts ?? [];
    for (const toast of toasts) {
      if (scheduled[toast.id]) continue;
      scheduled[toast.id] = true;
      const ms = toast.level === 'error' ? 6000 : 3500;
      setTimeout(() => void service.dispatch('view.dismiss_toast', { id: toast.id }), ms);
    }
  });

  /** The message: an i18n id when one exists, else the text itself. */
  function text(toast: Toast): string {
    return t(toast.message, toast.args ?? {});
  }
</script>

<div class="toasts" aria-live="polite" data-testid="toasts">
  {#each view.state?.toasts ?? [] as toast (toast.id)}
    <div class="toast {cls[toast.level]}" role={toast.level === 'error' ? 'alert' : 'status'}>
      <Icon name={icons[toast.level]} /><span>{text(toast)}</span>
    </div>
  {/each}
</div>
