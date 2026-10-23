<script lang="ts">
  // Import (design/ux.md flow 3): a file and what to do with its colours
  // (`document.import_data`, ./import.ts); the report comes back as a toast. An
  // image is selected and its raster dialog opens: how it is engraved comes next.
  import { app } from '../context';
  import type { ImportColors } from '../gen/api';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import DialogFrame from '../shell/DialogFrame.svelte';
  import { importFile, importFormat } from './import';

  let { onclose }: { onclose: () => void } = $props();
  const { service, run } = app();
  let file: File | null = $state(null);
  let colors: ImportColors = $state('keep');

  async function go() {
    const f = file;
    if (!f) return;
    await run(async () => {
      const r = await importFile(service, f.name, await f.arrayBuffer(), colors);
      onclose();
      await service.dispatch('view.toast', {
        level: r.unsupported ? 'warning' : 'ok',
        message: t('import-done', { objects: r.objects.length, layers: r.layers.length }),
      });
      if (importFormat(f.name) === 'image' && r.objects.length === 1) {
        await service.dispatch('document.select', { ids: r.objects });
        await service.dispatch('view.open_dialog', { name: 'raster' });
      }
    });
  }
</script>

<DialogFrame title={t('import-title')} testid="dialog-import" {onclose}>
  <div class="field">
    <label for="import-file">{t('import-file')}</label>
    <input
      id="import-file"
      type="file"
      accept=".svg,image/svg+xml,.dxf,image/png,image/jpeg,image/bmp,image/gif"
      data-testid="import-file"
      onchange={(e) => (file = e.currentTarget.files?.[0] ?? null)}
    />
  </div>
  <div class="field">
    <span class="mini">{t('import-colors')}</span>
    <div class="found" role="radiogroup" aria-label={t('import-colors')}>
      {#each ['keep', 'unify'] as const as v (v)}
        <label
          ><input type="radio" name="import-colors" value={v} bind:group={colors} /><span
            >{t(`import-colors-${v}`)}</span
          ></label
        >
      {/each}
    </div>
  </div>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t('preflight-cancel')}</button>
    <button class="btn primary" disabled={!file} data-testid="import-go" onclick={go}
      ><Icon name="import" /><span>{t('import-go')}</span></button
    >
  {/snippet}
</DialogFrame>
