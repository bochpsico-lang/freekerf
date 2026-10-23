<script lang="ts">
  // Over the Produce canvas when the design has objects and there is no job, or the
  // design changed after the job was made (update it, placed as before) or after
  // its G-code file was loaded (make the job from the design instead). Not while a
  // job runs or the alarm banner shows.
  import { app } from '../context';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import { designJob, designJobOffer } from './design-job.svelte';

  const { machine, doc, service, run } = app();
  const program = $derived(machine.state?.program ?? null);
  const inJob = $derived(!!machine.state?.machine.in_program);
  // the alarm banner's place (AlarmBanner.svelte)
  const alarm = $derived(
    machine.visual === 'alarm' || ((machine.state?.resume ?? null) !== null && machine.visual !== 'run'),
  );
  const offer = $derived(
    inJob || alarm
      ? null
      : designJobOffer(doc.summary?.objects.length ?? 0, doc.summary?.document_revision, program),
  );
  const file = $derived(offer === 'stale' && !program?.from_document);

  function make() {
    return offer === 'stale' ? designJob.remake(service, run, program) : designJob.make(service, run);
  }
</script>

{#if offer}
  <div class="banner info" role="status" data-testid="job-banner" data-offer={offer}>
    <span class="banner-ico"><Icon name="output" /></span>
    <div class="banner-text">
      <strong
        >{offer === 'none'
          ? t('job-banner-none')
          : file
            ? t('job-banner-stale-file', { name: program?.name ?? '' })
            : t('job-banner-stale')}</strong
      >
    </div>
    <div class="banner-actions">
      <button class="btn primary" data-testid="job-banner-go" disabled={designJob.busy} onclick={make}>
        <Icon name="output" /><span
          >{designJob.busy
            ? t('job-design-busy')
            : offer === 'none' || file
              ? t('job-banner-make')
              : t('job-banner-update')}</span
        >
      </button>
    </div>
  </div>
{/if}
