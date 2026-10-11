<script lang="ts">
  // The material test (design/ux.md flow 6): a grid of cells, power rising by row
  // and speed by column, each cell on a layer of its own, labelled in single-stroke
  // text (`document.material_test`, one undo step). The parameters are local to the
  // dialog; S of 100 % is the board's `$30`.
  import { app } from '../context';
  import type { TestMode } from '../gen/api';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import DialogFrame from '../shell/DialogFrame.svelte';
  import NumField from './NumField.svelte';

  let { onclose }: { onclose: () => void } = $props();
  const { service, run, machine, settings } = app();

  let mode: TestMode = $state('fill');
  let power = $state({ from: 10, to: 100, steps: 5 });
  let speed = $state({ from: 500, to: 6000, steps: 5 });
  let cell = $state(10);
  const inch = $derived(settings.ui.units === 'in');
  const cells = $derived(power.steps * speed.steps);
  const steps = (v: number) => Math.min(20, Math.max(1, Math.round(v)));

  async function go() {
    await run(async () => {
      const r = await service.dispatch('document.material_test', {
        mode,
        power_pct: $state.snapshot(power),
        speed_mm_min: $state.snapshot(speed),
        cell_mm: cell,
        max_s: machine.info?.max_power || 1000,
      });
      onclose();
      await service.dispatch('view.toast', { level: 'ok', message: t('mtest-done', { n: r.layers.length }) });
    });
  }
</script>

<DialogFrame title={t('mtest-title')} testid="dialog-material-test" {onclose}>
  <div class="field">
    <span class="mini">{t('mtest-mode')}</span>
    <div class="seg" role="radiogroup" aria-label={t('mtest-mode')}>
      {#each ['fill', 'line'] as const as m (m)}
        <button role="radio" aria-checked={mode === m} data-mode={m} onclick={() => (mode = m)}
          >{t(`mtest-mode-${m}`)}</button
        >
      {/each}
    </div>
  </div>
  <span class="mini">{t('mtest-power')}</span>
  <div class="fgrid">
    <NumField
      label={t('mtest-from')}
      value={power.from}
      unit="%"
      decimals={0}
      min={0}
      max={100}
      testid="mtest-power-from"
      oncommit={(v) => (power.from = Math.min(100, Math.max(0, v)))}
    />
    <NumField
      label={t('mtest-to')}
      value={power.to}
      unit="%"
      decimals={0}
      min={0}
      max={100}
      testid="mtest-power-to"
      oncommit={(v) => (power.to = Math.min(100, Math.max(0, v)))}
    />
    <NumField
      label={t('mtest-steps')}
      value={power.steps}
      decimals={0}
      min={1}
      max={20}
      testid="mtest-power-steps"
      oncommit={(v) => (power.steps = steps(v))}
    />
  </div>
  <span class="mini">{t('mtest-speed')}</span>
  <div class="fgrid">
    <NumField
      label={t('mtest-from')}
      value={speed.from}
      unit="mm/min"
      decimals={0}
      min={1}
      testid="mtest-speed-from"
      oncommit={(v) => (speed.from = Math.max(1, v))}
    />
    <NumField
      label={t('mtest-to')}
      value={speed.to}
      unit="mm/min"
      decimals={0}
      min={1}
      testid="mtest-speed-to"
      oncommit={(v) => (speed.to = Math.max(1, v))}
    />
    <NumField
      label={t('mtest-steps')}
      value={speed.steps}
      decimals={0}
      min={1}
      max={20}
      testid="mtest-speed-steps"
      oncommit={(v) => (speed.steps = steps(v))}
    />
  </div>
  <NumField
    label={t('mtest-cell')}
    value={cell}
    unit={inch ? 'in' : 'mm'}
    scale={inch ? 25.4 : 1}
    min={1}
    testid="mtest-cell"
    oncommit={(v) => (cell = Math.max(1, v))}
  />
  <p class="hint" data-testid="mtest-cells">{t('mtest-cells', { n: cells })}</p>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t('preflight-cancel')}</button>
    <button class="btn primary" data-testid="mtest-go" onclick={go}
      ><Icon name="test" /><span>{t('mtest-go')}</span></button
    >
  {/snippet}
</DialogFrame>
