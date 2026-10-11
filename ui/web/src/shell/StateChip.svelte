<script lang="ts">
  // Machine state: colour + icon + word (never colour alone); stripes while running,
  // blinking on alarm (design/ux.md "Estados da máquina"). Opens the machine panel.
  import { app } from '../context';
  import type { IconName } from '../gen/icons';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';

  const { machine, service } = app();
  const icons: Record<string, IconName> = {
    offline: 'plugOff',
    connecting: 'plug',
    idle: 'check',
    run: 'laser',
    hold: 'pause',
    alarm: 'warn',
  };
</script>

<button
  class="state-chip"
  data-state={machine.visual}
  data-testid="state-chip"
  aria-live="polite"
  onclick={() => void service.dispatch('view.open_panel', { name: 'machine' })}
>
  <span class="state-ico"><Icon name={icons[machine.visual] ?? 'dot'} /></span>
  <span class="state-label">{t(`state-${machine.visual}`)}</span>
</button>
