<script lang="ts">
  // The machine panel of the Produce mode (design/design-system.md "Painel da
  // máquina"): connection, job, overrides, jog, custom buttons, laser module.
  // Machine-tier actions are dispatched directly; anything that can fire the laser
  // goes through `laser()` (fire confirmation).
  import { app } from '../context';
  import type { Anchor, JobStart, SerialPortInfo } from '../gen/api';
  import type { IconName } from '../gen/icons';
  import { t } from '../i18n/index.svelte';
  import Icon from '../lib/Icon.svelte';
  import { formatDuration, formatLength } from '../lib/format';
  import { JOG_STEPS, progress } from './job';
  import JogPad from './JogPad.svelte';
  import PortPermissionWarning from './PortPermissionWarning.svelte';
  import { ActionFailure } from '../service';

  const { machine, preview, settings, service, run, laser, doc } = app();
  const EMULATOR = '__emulator__';

  let ports: SerialPortInfo[] = $state([]);
  let port = $state(EMULATOR);
  /** The last connection refused for permissions (also when listing said `ok`). */
  let refused: { port: string; group?: string | null } | null = $state(null);
  let fileInput: HTMLInputElement;
  /** How the job made from the design is placed: local choices of the panel. */
  let start: JobStart = $state('absolute');
  let anchor: Anchor = $state('bottom_left');
  const STARTS: JobStart[] = ['absolute', 'current_position', 'user_origin'];
  /** The 3×3 grid, top row first. */
  const ANCHORS: Anchor[] = [
    'top_left',
    'top',
    'top_right',
    'left',
    'center',
    'right',
    'bottom_left',
    'bottom',
    'bottom_right',
  ];

  const m = $derived(machine.state?.machine);
  const connected = $derived(!!m?.connected);
  const idle = $derived(machine.visual === 'idle');
  const running = $derived(machine.visual === 'run' && !!m?.in_program);
  const paused = $derived(machine.visual === 'hold');
  const program = $derived(machine.state?.program ?? null);
  const job = $derived(progress(m));
  const units = $derived(settings.ui.units);
  // the 60 Hz head stream, else the last state read
  const head = $derived(preview.head ?? m?.work_position ?? null);
  const step = $derived(settings.settings?.jog?.step_mm ?? 1);
  const continuous = $derived(settings.settings?.machine?.continuous_jog ?? false);
  const buttons = $derived(settings.settings?.buttons ?? []);

  async function listPorts() {
    ports = await service.dispatch('machine.list_ports', {});
    const saved = settings.settings?.machine?.port;
    if (saved && ports.some((p) => p.path === saved)) port = saved;
  }
  $effect(() => {
    void listPorts().catch(() => undefined);
  });

  /** The selected port when this user may not open it: the warning under the picker. */
  const denied = $derived.by(() => {
    if (connected || port === EMULATOR) return null;
    if (refused?.port === port) return refused;
    const info = ports.find((p) => p.path === port);
    return info?.access === 'denied' ? { port, group: info.group } : null;
  });

  function connect() {
    return run(async () => {
      if (connected) return service.dispatch('machine.disconnect', {});
      if (port === EMULATOR) return service.dispatch('machine.connect', { emulator: true });
      try {
        refused = null;
        return await service.dispatch('machine.connect', { port });
      } catch (e) {
        const cause = e instanceof ActionFailure && e.error.kind === 'failed' ? e.error.message.cause : null;
        // the warning says it, with the fix: no toast
        if (cause?.code === 'port_permission_denied') refused = { port: cause.port, group: cause.group };
        else throw e;
      }
    });
  }

  async function openFile(e: Event & { currentTarget: HTMLInputElement }) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (!file) return;
    const gcode = await file.text();
    await run(() => service.dispatch('job.load_text', { name: file.name, gcode }));
  }

  function override(index: 0 | 1 | 2, value: number) {
    const cur = m?.override_targets ?? [100, 100, 100];
    const next: [number, number, number] = [cur[0], cur[1], cur[2]];
    next[index] = value;
    return run(() =>
      service.dispatch('machine.overrides', { feed: next[0], rapid: next[1], power: next[2] }),
    );
  }
</script>

<div class="machine" class:offline={!connected} data-testid="machine-panel">
  <!-- connection -->
  <div class="group conn">
    <div class="field-row">
      <label for="port">{t('machine-port')}</label>
      <select id="port" bind:value={port} disabled={connected} data-testid="port">
        <option value={EMULATOR}>{t('machine-emulator')}</option>
        {#each ports as p (p.path)}<option value={p.path}>{p.path}</option>{/each}
      </select>
      <button
        class="icon-btn"
        title={t('machine-refreshPorts')}
        aria-label={t('machine-refreshPorts')}
        onclick={() => run(listPorts)}
        disabled={connected}
      >
        <Icon name="repeat" />
      </button>
    </div>
    {#if denied}<PortPermissionWarning port={denied.port} group={denied.group} />{/if}
    <button class="btn wide" data-testid="connect" onclick={connect}>
      <Icon name={connected ? 'plugOff' : 'plug'} /><span
        >{t(connected ? 'machine-disconnect' : 'machine-connect')}</span
      >
    </button>
    {#if connected && machine.state?.firmware_version}
      <p class="hint" data-testid="firmware">
        {t('machine-detected', {
          fw: `${m?.firmware ?? ''} ${machine.state.firmware_version}`,
          w: machine.info?.table_width_mm ?? '?',
          h: machine.info?.table_height_mm ?? '?',
        })}
      </p>
    {/if}
  </div>

  <!-- job -->
  <div class="group job">
    <div class="job-head">
      <h3>{t('machine-job')}</h3>
      <span class="job-est"
        ><Icon name="clock" /><span data-testid="job-estimate"
          >{program ? formatDuration(program.estimated_ms) : '—'}</span
        ></span
      >
    </div>
    <div class="file-row">
      <button class="btn" data-testid="open-gcode" onclick={() => fileInput.click()}>
        <Icon name="folder" /><span>{t('machine-openGcode')}</span>
      </button>
      <span class="hint mono" data-testid="program-name"
        >{program ? `${program.name} · ${program.lines}` : t('machine-noProgram')}</span
      >
      <input
        bind:this={fileInput}
        type="file"
        accept=".nc,.gcode,.gc,.ngc,.tap,.txt,.cnc"
        hidden
        data-testid="gcode-file"
        onchange={openFile}
      />
    </div>
    {#if doc.objects.length}
      <!-- the job from the design: where it starts and which point goes there -->
      <div class="design-job" data-testid="design-job">
        <span class="mini">{t('job-start')}</span>
        <div class="seg" role="radiogroup" aria-label={t('job-start')}>
          {#each STARTS as s (s)}
            <button role="radio" aria-checked={start === s} data-start={s} onclick={() => (start = s)}
              >{t(`job-start-${s}`)}</button
            >
          {/each}
        </div>
        <div class="anchor-row">
          <div class="anchor-grid" role="radiogroup" aria-label={t('job-anchor')}>
            {#each ANCHORS as a (a)}
              <button
                class="anchor"
                role="radio"
                aria-checked={anchor === a}
                aria-label={t(`job-anchor-${a}`)}
                title={t(`job-anchor-${a}`)}
                data-anchor={a}
                disabled={start === 'absolute'}
                onclick={() => (anchor = a)}
              ></button>
            {/each}
          </div>
          <button
            class="btn"
            data-testid="design-job-go"
            onclick={() => run(() => service.dispatch('job.from_document', { start, anchor }))}
            ><Icon name="output" /><span>{t('job-design-go')}</span></button
          >
        </div>
      </div>
    {/if}
    <div
      class="progress"
      role="progressbar"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={job?.pct ?? 0}
      data-testid="progress"
    >
      <div class="bar" style:width="{job?.pct ?? 0}%"></div>
      <span class="pct">{job?.pct ?? 0}%</span>
    </div>
    <dl class="job-stats">
      <div>
        <dt>{t('machine-elapsed')}</dt>
        <dd>{job ? formatDuration(job.elapsedMs) : '—'}</dd>
      </div>
      <div>
        <dt>{t('machine-remaining')}</dt>
        <dd data-testid="remaining">{job ? formatDuration(job.remainingMs) : '—'}</dd>
      </div>
      <div>
        <dt>{t('machine-lines')}</dt>
        <dd>{job ? `${job.executed}/${job.target}` : '—'}</dd>
      </div>
    </dl>
    <div class="job-actions">
      <button
        class="btn primary big"
        data-testid="start"
        disabled={!idle || !program}
        onclick={() => laser({ action: 'job.run', params: {} })}
      >
        <Icon name="play" /><span>{t('machine-start')}</span>
      </button>
      {#if paused}
        <button
          class="btn big"
          data-testid="resume"
          onclick={() => run(() => service.dispatch('machine.resume', {}))}
        >
          <Icon name="play" /><span>{t('machine-resume')}</span>
        </button>
      {:else}
        <button
          class="btn big"
          data-testid="pause"
          disabled={!running}
          onclick={() => void service.dispatch('machine.feed_hold', {})}
        >
          <Icon name="pause" /><span>{t('machine-pause')}</span>
        </button>
      {/if}
      <button
        class="btn big danger-outline"
        data-testid="stop-job"
        disabled={!m?.in_program}
        onclick={() => void service.dispatch('job.abort', {})}
      >
        <Icon name="stop" /><span>{t('machine-stop')}</span>
      </button>
    </div>
    <button
      class="btn wide"
      data-testid="frame"
      title={t('machine-frameHint')}
      disabled={!idle || !program?.drawing_range}
      onclick={() => run(() => service.dispatch('job.frame', {}))}
    >
      <Icon name="frame" /><span>{t('machine-frame')}</span>
    </button>
  </div>

  <!-- overrides -->
  <div class="group overrides">
    <h3>{t('machine-overrides')}</h3>
    {#each [[0, 'machine-ovFeed', 10, 200], [2, 'machine-ovPower', 10, 200]] as const as [i, label, min, max] (i)}
      <div class="slider">
        <label class="mini" for="ov{i}">{t(label)}</label>
        <input
          id="ov{i}"
          type="range"
          {min}
          {max}
          step="10"
          value={m?.override_targets[i] ?? 100}
          disabled={!connected}
          onchange={(e) => override(i, Number(e.currentTarget.value))}
        />
        <span class="mono">{m?.overrides[i] ?? 100}%</span>
      </div>
    {/each}
    <div class="field-row">
      <span class="mini">{t('machine-ovRapid')}</span>
      <div class="seg" role="radiogroup" aria-label={t('machine-ovRapid')}>
        {#each [25, 50, 100] as v (v)}
          <button
            role="radio"
            aria-checked={m?.override_targets[1] === v}
            class:on={m?.override_targets[1] === v}
            disabled={!connected}
            onclick={() => override(1, v)}>{v}%</button
          >
        {/each}
      </div>
    </div>
  </div>

  <!-- jog -->
  <div class="group jog">
    <div class="dro" aria-live="off">
      <div><span>X</span><b data-testid="dro-x">{head ? formatLength(head.x, units) : '—'}</b></div>
      <div><span>Y</span><b data-testid="dro-y">{head ? formatLength(head.y, units) : '—'}</b></div>
    </div>
    <div class="jog-wrap">
      <JogPad enabled={!!m?.jog_enabled || machine.visual === 'idle'} {continuous} {step} />
      <div class="jog-side">
        <span class="mini">{t('machine-step')}</span>
        <div class="seg steps" role="radiogroup" aria-label={t('machine-step')}>
          {#each JOG_STEPS as s (s)}
            <button
              role="radio"
              aria-checked={step === s}
              class:on={step === s}
              onclick={() => run(() => service.dispatch('settings.set', { key: 'jog.step_mm', value: s }))}
              >{s}</button
            >
          {/each}
        </div>
        <label class="check"
          ><input
            type="checkbox"
            checked={continuous}
            data-testid="jog-continuous"
            onchange={(e) =>
              run(() =>
                service.dispatch('settings.set', {
                  key: 'machine.continuous_jog',
                  value: e.currentTarget.checked,
                }),
              )}
          /><span>{t('machine-continuous')}</span></label
        >
        <div class="mach-btns">
          <button
            class="btn"
            data-testid="home"
            disabled={!connected}
            onclick={() => run(() => service.dispatch('machine.home', {}))}
          >
            <Icon name="home" /><span>{t('machine-home')}</span>
          </button>
          <button
            class="btn"
            data-testid="unlock"
            disabled={!connected}
            onclick={() => run(() => service.dispatch('machine.unlock', {}))}
          >
            <Icon name="unlock" /><span>{t('machine-unlock')}</span>
          </button>
          <button
            class="btn"
            data-testid="set-origin"
            disabled={!idle}
            onclick={() => run(() => service.dispatch('machine.set_zero', {}))}
          >
            <Icon name="origin" /><span>{t('machine-setOrigin')}</span>
          </button>
        </div>
      </div>
    </div>
    {#if !connected}<p class="hint">{t('machine-notConnected')}</p>{/if}
  </div>

  <!-- custom buttons -->
  <div class="group">
    <h3>{t('machine-custom')}</h3>
    {#if buttons.length}
      <div class="custom-btns">
        {#each buttons as b, i (i)}
          <button
            class="btn"
            disabled={!connected}
            onclick={() => laser({ action: 'machine.custom_code', params: { code: b.code ?? '' } })}
          >
            <Icon name={(b.icon ?? 'bolt') as IconName} /><span>{b.label}</span>
          </button>
        {/each}
      </div>
    {:else}
      <p class="hint">{t('machine-noButtons')}</p>
    {/if}
  </div>

  <!-- laser module -->
  {#if machine.info?.laser_life.length}
    <div class="group">
      <h3>{t('machine-laserLife')}</h3>
      {#each machine.info.laser_life as c, i (c.id)}
        <p class="hint mono" class:on={machine.info.laser_in_use === i}>
          {c.name} · {formatDuration(c.run_ms)}
          {machine.info.laser_in_use === i ? '●' : ''}
        </p>
      {/each}
    </div>
  {/if}
</div>

<style>
  .design-job {
    display: grid;
    gap: var(--fk-space-2);
  }
  .anchor-row {
    display: flex;
    align-items: center;
    gap: var(--fk-space-3);
  }
  .anchor-grid {
    display: grid;
    grid-template-columns: repeat(3, 14px);
    gap: 3px;
  }
  .anchor {
    width: 14px;
    height: 14px;
    padding: 0;
    border: 1px solid var(--fk-color-border-strong);
    border-radius: 2px;
    background: transparent;
  }
  .anchor[aria-checked='true'] {
    background: var(--fk-color-accent);
    border-color: var(--fk-color-accent);
  }
  .anchor:disabled {
    opacity: 0.4;
  }
  .file-row {
    display: flex;
    align-items: center;
    gap: var(--fk-space-2);
    min-width: 0;
  }
  .file-row .hint {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
