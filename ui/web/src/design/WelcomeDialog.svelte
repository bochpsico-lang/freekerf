<script lang="ts">
  // Welcome (design/ux.md flow 1 and 2): set up the machine, import from LaserGRBL
  // (`migrate.detect` / `migrate.run`: the LaserGRBL files are only read), or
  // explore with the emulator.
  import type { MigrationItemKind, MigrationReport, MigrationScan } from '../contract/document';
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import DialogFrame from '../shell/DialogFrame.svelte';

  let { onclose }: { onclose: () => void } = $props();
  const { doc, service, run } = app();

  type Step = 'home' | 'migrate' | 'ready';
  let step: Step = $state('home');
  let scan: MigrationScan | null = $state(null);
  let chosen: Record<string, boolean> = $state({});
  let report: MigrationReport | null = $state(null);

  async function migrate() {
    if (!doc) return;
    step = 'migrate';
    await run(async () => {
      scan = await doc.service.dispatch('migrate.detect', {});
      chosen = Object.fromEntries(scan.items.map((i) => [i.kind, true]));
    });
  }

  async function runMigration() {
    const d = doc;
    if (!d || !scan) return;
    const items = scan.items.map((i) => i.kind).filter((k) => chosen[k]) as MigrationItemKind[];
    await run(async () => {
      report = await d.service.dispatch('migrate.run', { items });
      step = 'ready';
    });
  }

  function explore() {
    onclose();
    void run(() => service.dispatch('machine.connect', { emulator: true }));
  }

  function setup() {
    onclose();
    void run(() => service.dispatch('view.open_dialog', { name: 'settings' }));
  }
</script>

<DialogFrame title={t('welcome-title')} testid="dialog-welcome" wide {onclose}>
  <div class="wel" data-step={step}>
    {#if step === 'home'}
      <div class="wel-hero">
        <div>
          <h2>{t('welcome-title')}</h2>
          <p>{t('welcome-sub')}</p>
        </div>
      </div>
      <div class="wel-actions">
        <button class="wel-card" onclick={setup}
          ><Icon name="machine" /><b>{t('welcome-setup')}</b><small
            >{t('welcome-kind-diode')} · {t('welcome-kind-co2')}</small
          ></button
        >
        {#if doc}
          <button class="wel-card" data-testid="welcome-migrate" onclick={migrate}
            ><Icon name="import" /><b>{t('welcome-migrate')}</b><small>{t('welcome-migrateSub')}</small
            ></button
          >
        {/if}
        <button class="wel-card" onclick={explore}
          ><Icon name="sparkle" /><b>{t('welcome-explore')}</b><small>{t('machine-emulator')}</small></button
        >
      </div>
      <div class="wel-foot">
        <p class="wel-origin">{t('welcome-origin')}</p>
        <button class="btn" onclick={onclose}>{t('welcome-skip')}</button>
      </div>
    {:else if step === 'migrate'}
      <h2>{t('welcome-migrateTitle')}</h2>
      {#if !scan}
        <p class="wel-search">{t('welcome-searching')}</p>
      {:else if !scan.found}
        <p data-testid="migrate-none">{t('migrate-none')}</p>
      {:else}
        <p>{t('welcome-migrateNote')}</p>
        <p class="hint mono">{scan.path}</p>
        <div class="found" data-testid="migrate-items">
          {#each scan.items as item (item.kind)}
            <label
              ><input type="checkbox" bind:checked={chosen[item.kind]} /><span
                >{t(`migrate-${item.kind}`, { n: item.count })}</span
              ></label
            >
          {/each}
        </div>
      {/if}
      <div class="wel-foot">
        <button class="btn" onclick={() => (step = 'home')}
          ><Icon name="chevLeft" /><span>{t('welcome-back')}</span></button
        >
        <button
          class="btn primary"
          data-testid="migrate-go"
          disabled={!scan?.found || !Object.values(chosen).some(Boolean)}
          onclick={runMigration}><span>{t('welcome-migrateGo')}</span><Icon name="chevRight" /></button
        >
      </div>
    {:else}
      <div class="wel-hero">
        <div>
          <h2>{t('welcome-ready')}</h2>
          {#if report}
            <ul data-testid="migrate-report">
              {#each report.imported as i (i.kind)}
                <li>{t(`migrate-${i.kind}`, { n: i.count })}</li>
              {/each}
            </ul>
            {#each report.warnings as w (w)}<p class="hint">{w}</p>{/each}
          {/if}
        </div>
      </div>
      <div class="wel-foot">
        <span></span>
        <button class="btn primary" onclick={onclose}><span>{t('welcome-finish')}</span></button>
      </div>
    {/if}
  </div>
</DialogFrame>
