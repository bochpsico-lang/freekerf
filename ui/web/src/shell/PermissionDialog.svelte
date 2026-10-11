<script lang="ts">
  // A script or an agent asks for an action above its tier (PLAN §9.5): the person
  // answers here, never the script or the agent (`answerPermission` is not an
  // action). Unanswered requests are denied after two minutes, in Rust.
  import { app } from '../context';
  import type { GrantScope, PendingPermission } from '../gen/api';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import DialogFrame from './DialogFrame.svelte';

  let { request }: { request: PendingPermission } = $props();
  const { service, run } = app();

  const answer = (scope: GrantScope | null) => run(() => service.answerPermission(request.id, scope));
</script>

<DialogFrame title={t('perm-title')} testid="dialog-permission" closable={false} onclose={() => answer(null)}>
  <p data-testid="perm-text">
    {t(request.actor.kind === 'agent' ? 'perm-agent' : 'perm-macro', {
      name: request.actor.name,
      action: request.action,
    })}
  </p>
  <p class="hint">{t('perm-tier', { tier: request.tier })}</p>
  {#snippet footer()}
    <button class="btn" data-testid="perm-deny" onclick={() => answer(null)}
      ><Icon name="close" /><span>{t('perm-deny')}</span></button
    >
    <button class="btn" data-testid="perm-run" onclick={() => answer('run')}>{t('perm-run')}</button>
    <button class="btn" data-testid="perm-session" onclick={() => answer('session')}
      >{t('perm-session')}</button
    >
    <button class="btn primary" data-testid="perm-always" onclick={() => answer('always')}
      >{t('perm-always')}</button
    >
  {/snippet}
</DialogFrame>
