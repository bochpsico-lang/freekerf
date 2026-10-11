<script lang="ts">
  // 3×3 jog pad (design/interaction.md §5). Step mode: one step per press.
  // Continuous mode is a dead-man control: it moves while the button is held and
  // sends the jog cancel on release, when the pointer leaves or is lost, and when
  // the window loses the focus.
  import { onDestroy } from 'svelte';

  import { app } from '../context';
  import type { IconName } from '../gen/icons';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';

  let { enabled, continuous, step }: { enabled: boolean; continuous: boolean; step: number } = $props();
  const { service, run } = app();

  const pad: [string, IconName, string][] = [
    ['NW', 'upLeft', '↖'],
    ['N', 'up', 'jog-n'],
    ['NE', 'upRight', '↗'],
    ['W', 'left', 'jog-w'],
    ['Home', 'home', 'machine-goOrigin'],
    ['E', 'right', 'jog-e'],
    ['SW', 'downLeft', '↙'],
    ['S', 'down', 'jog-s'],
    ['SE', 'downRight', '↘'],
  ];

  let held: string | null = $state(null);

  function release() {
    if (held === null) return;
    held = null;
    // safety: always allowed, sent even if the press failed
    void service.dispatch('machine.jog_stop', {});
  }

  function press(e: PointerEvent, direction: string) {
    if (!enabled) return;
    if (continuous && direction !== 'Home') {
      (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
      held = direction;
      void run(() => service.dispatch('machine.jog', { direction }));
    } else {
      void run(() => service.dispatch('machine.jog', { direction, step_mm: step }));
    }
  }

  const blur = () => release();
  window.addEventListener('blur', blur);
  onDestroy(() => {
    window.removeEventListener('blur', blur);
    release();
  });
</script>

<div class="jogpad" data-testid="jogpad">
  {#each pad as [direction, icon, label] (direction)}
    <button
      data-jog={direction}
      class:held={held === direction}
      disabled={!enabled}
      title={label.includes('-') ? t(label) : label}
      aria-label={label.includes('-') ? t(label) : direction}
      onpointerdown={(e) => press(e, direction)}
      onpointerup={release}
      onpointercancel={release}
      onlostpointercapture={release}
      onpointerleave={() => {
        if (held === direction) release();
      }}
      onkeydown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          if (enabled) void run(() => service.dispatch('machine.jog', { direction, step_mm: step }));
        }
      }}
    >
      <Icon name={icon} />
    </button>
  {/each}
</div>
