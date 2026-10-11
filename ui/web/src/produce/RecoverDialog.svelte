<script lang="ts">
  // Job recovery (LaserGRBL's ResumeJobForm): where to go on from, home first,
  // restore the work origin. Going on fires the laser: it goes through the fire
  // confirmation (`job.resume`, tier laser).
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import DialogFrame from '../shell/DialogFrame.svelte';
  import { causeId, resumeOptions, type ResumeFrom } from './job';

  let { onclose }: { onclose: () => void } = $props();
  const { machine, laser } = app();
  const resume = $derived(machine.state?.resume ?? null);
  const options = $derived(resume ? resumeOptions(resume) : []);

  let chosen: ResumeFrom | null = $state(null);
  let line = $state(1);
  let homing: boolean | null = $state(null);
  let setWco: boolean | null = $state(null);

  const from = $derived(chosen ?? options.find((o) => o.recommended)?.from ?? 'start');
  const position = $derived(
    from === 'line'
      ? Math.max(0, Math.floor(line) - 1)
      : (options.find((o) => o.from === from)?.position ?? 0),
  );

  function go() {
    if (!resume) return;
    onclose();
    void laser({
      action: 'job.resume',
      params: { position, homing: homing ?? resume.homing, set_wco: setWco ?? resume.set_wco },
    });
  }
</script>

<DialogFrame title={t('recover-title')} testid="dialog-recover" {onclose}>
  {#if resume}
    <p class="hint"><b>{t('recover-cause')}</b>: {t(causeId(resume.issue))}</p>
    <div class="radios" role="radiogroup">
      {#each options as o (o.from)}
        <label>
          <input
            type="radio"
            name="from"
            value={o.from}
            checked={from === o.from}
            onchange={() => (chosen = o.from)}
          />
          {#if o.from === 'start'}{t('recover-fromStart')}
          {:else if o.from === 'before'}{t('recover-fromBefore', { n: o.position + 1 })}
          {:else if o.from === 'sent'}{t('recover-fromSent', { n: o.position + 1 })}
          {:else}{t('recover-fromLine')}
            <span class="num"
              ><input
                type="number"
                min="1"
                max={resume.target}
                bind:value={line}
                aria-label={t('recover-fromLine')}
              /></span
            >
          {/if}
          {#if o.recommended}<span class="tag">{t('recover-recommended')}</span>{/if}
        </label>
      {/each}
    </div>
    <label class="check"
      ><input
        type="checkbox"
        checked={homing ?? resume.homing}
        onchange={(e) => (homing = e.currentTarget.checked)}
      /><span>{t('recover-homing')}</span></label
    >
    <label class="check"
      ><input
        type="checkbox"
        checked={setWco ?? resume.set_wco}
        onchange={(e) => (setWco = e.currentTarget.checked)}
      /><span>{t('recover-restoreWco')}</span></label
    >
  {:else}
    <p class="hint">{t('recover-nothing')}</p>
  {/if}
  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t('recover-dismiss')}</button>
    <button class="btn primary" data-testid="recover-go" disabled={!resume} onclick={go}>
      <Icon name="play" /><span>{t('recover-go')}</span>
    </button>
  {/snippet}
</DialogFrame>
