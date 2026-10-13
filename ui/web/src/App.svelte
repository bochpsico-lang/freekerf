<script lang="ts">
  // The app shell: top bar (state + STOP, always visible), tool rail, canvas, dock,
  // status bar, bottom bar (narrow), dialogs and toasts. Layout by window width,
  // density by input kind, theme from the settings; everything else from the
  // view state in Rust (PLAN §9: the UI changes app state only through actions).
  import { setApp, type AppContext, type LaserRequest } from './context';
  import ImportDialog from './design/ImportDialog.svelte';
  import RasterDialog from './design/RasterDialog.svelte';
  import MaterialTestDialog from './design/MaterialTestDialog.svelte';
  import LayersPanel from './design/LayersPanel.svelte';
  import ObjectPanel from './design/ObjectPanel.svelte';
  import WelcomeDialog from './design/WelcomeDialog.svelte';
  import { setLocale, t } from './i18n/index.svelte';
  import ConsolePanel from './produce/ConsolePanel.svelte';
  import FireConfirmation from './produce/FireConfirmation.svelte';
  import MachinePanel from './produce/MachinePanel.svelte';
  import RecoverDialog from './produce/RecoverDialog.svelte';
  import { progress } from './produce/job';
  import Icon from './lib/Icon.svelte';
  import type { AppService } from './service';
  import BottomBar from './shell/BottomBar.svelte';
  import Dialogs from './shell/Dialogs.svelte';
  import Dock from './shell/Dock.svelte';
  import DockRail from './shell/DockRail.svelte';
  import { isTyping, shortcut, type Command } from './shell/shortcuts';
  import Stage from './shell/Stage.svelte';
  import StatusBar from './shell/StatusBar.svelte';
  import Toasts from './shell/Toasts.svelte';
  import ToolRail from './shell/ToolRail.svelte';
  import TopBar from './shell/TopBar.svelte';
  import { resolveDensity, type Environment } from './state/environment.svelte';
  import type { AutomationProjection } from './state/automation.svelte';
  import type { ConsoleProjection } from './state/console.svelte';
  import PermissionDialog from './shell/PermissionDialog.svelte';
  import type { DocumentProjection } from './state/document.svelte';
  import type { MachineState } from './state/machine.svelte';
  import type { PreviewState } from './state/preview.svelte';
  import type { SettingsProjection } from './state/settings.svelte';
  import type { ViewProjection } from './state/view.svelte';

  let {
    service,
    machine,
    preview,
    view,
    settings,
    env,
    console: consoleRows,
    doc,
    automation,
  }: {
    service: AppService;
    machine: MachineState;
    preview: PreviewState;
    view: ViewProjection;
    settings: SettingsProjection;
    env: Environment;
    console: ConsoleProjection;
    doc: DocumentProjection;
    automation: AutomationProjection;
  } = $props();

  /** Laser actions only ask: the app opens the fire confirmation (PLAN §9.5). */
  async function laser(request: LaserRequest): Promise<void> {
    await run(() => service.dispatch('fire.request', { action: request.action, params: request.params }));
  }

  async function run(action: () => Promise<unknown>): Promise<void> {
    try {
      await action();
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      await service.dispatch('view.toast', { level: 'error', message }).catch(() => undefined);
    }
  }

  // the projections are created once by start.ts and never replaced
  // svelte-ignore state_referenced_locally
  const ctx: AppContext = {
    service,
    machine,
    preview,
    view,
    settings,
    env,
    console: consoleRows,
    doc,
    automation,
    run,
    laser,
  };
  setApp(ctx);

  let stage: Stage | undefined = $state();
  let zoom = $state(1);
  // the narrow layout is machine-only (design/ux.md "Layouts")
  const mode = $derived(env.layout === 'narrow' ? 'produce' : view.mode);

  $effect(() => setLocale(settings.ui.language));

  // the first run opens the welcome by itself (once; closing it records it)
  let welcomeOffered = false;
  $effect(() => {
    if (!settings.firstRun || welcomeOffered) return;
    welcomeOffered = true;
    void run(() => service.dispatch('view.open_dialog', { name: 'welcome' }));
  });
  function closeWelcome(close: () => void) {
    close();
    if (!settings.ui.welcomed)
      void run(() => service.dispatch('settings.set', { key: 'ui.welcomed', value: true }));
  }

  $effect(() => {
    const root = document.documentElement;
    const ui = settings.ui;
    root.dataset.theme = ui.theme;
    if (ui.high_contrast) root.dataset.contrast = 'high';
    else delete root.dataset.contrast;
    root.dataset.density = resolveDensity(ui.density, env.input);
    root.dataset.layout = env.layout;
    root.dataset.mode = mode;
  });

  function exec(c: Command, e?: KeyboardEvent): void {
    const status = machine.state?.machine.status;
    switch (c) {
      case 'stop':
        void service.dispatch('machine.stop', {});
        return;
      case 'pause':
        if (status === 'Hold' || status === 'Door') void run(() => service.dispatch('machine.resume', {}));
        else if (machine.visual === 'run') void service.dispatch('machine.feed_hold', {});
        return;
      case 'start':
        if (machine.visual === 'idle' && machine.state?.program)
          void laser({ action: 'job.run', params: {} });
        return;
      case 'frame':
        if (machine.visual === 'idle') void run(() => service.dispatch('job.frame', {}));
        return;
      case 'mode-design':
        if (env.layout !== 'narrow') void run(() => service.dispatch('view.set_mode', { mode: 'design' }));
        return;
      case 'mode-produce':
        void run(() => service.dispatch('view.set_mode', { mode: 'produce' }));
        return;
      case 'fit':
        void run(() => service.dispatch('view.fit', {}));
        return;
      case 'zoom-in':
        stage?.zoom(1.25);
        return;
      case 'zoom-out':
        stage?.zoom(0.8);
        return;
      case 'close-dialog':
        // closing the fire confirmation cancels its request (also in Rust)
        void run(() =>
          view.topDialog === 'fire_confirmation'
            ? service.dispatch('fire.cancel', {})
            : service.dispatch('view.close_dialog', {}),
        );
        return;
      case 'shortcuts':
        void run(() => service.dispatch('view.open_dialog', { name: 'shortcuts' }));
        return;
    }
    editCommand(c, doc, e);
  }

  /** Design mode keys (only reached while the Design mode is shown). */
  function editCommand(c: Command, d: DocumentProjection, e?: KeyboardEvent): void {
    const ids = d.selection;
    const mods = { shift: !!e?.shiftKey, alt: !!e?.altKey };
    const tool = (tool: 'select' | 'rect' | 'ellipse' | 'text') =>
      void run(() => service.dispatch('view.set_tool', { tool }));
    switch (c) {
      case 'tool-select':
        return tool('select');
      case 'tool-rect':
        return tool('rect');
      case 'tool-ellipse':
        return tool('ellipse');
      case 'tool-text':
        return tool('text');
      case 'import':
        void run(() => service.dispatch('view.open_dialog', { name: 'import' }));
        return;
      case 'undo':
        void run(() => d.service.dispatch('history.undo', {}));
        return;
      case 'redo':
        void run(() => d.service.dispatch('history.redo', {}));
        return;
      case 'select-all':
        void run(() => d.service.dispatch('document.select_all', {}));
        return;
      case 'deselect':
        if (view.tool !== 'select') tool('select');
        else if (ids.length) void run(() => d.service.dispatch('document.clear_selection', {}));
        return;
      case 'delete':
      case 'duplicate':
      case 'copy':
      case 'cut':
      case 'paste':
      case 'combine':
      case 'split':
        void stage?.edit(c);
        return;
      case 'nudge-left':
        void stage?.nudge(-1, 0, mods);
        return;
      case 'nudge-right':
        void stage?.nudge(1, 0, mods);
        return;
      case 'nudge-up':
        void stage?.nudge(0, 1, mods);
        return;
      case 'nudge-down':
        void stage?.nudge(0, -1, mods);
        return;
    }
  }

  function keydown(e: KeyboardEvent): void {
    const c = shortcut(e, {
      // the context menu keeps its keys (only STOP and the function keys pass)
      typing: isTyping(e.target) || !!(e.target as Element | null)?.closest?.('[role="menu"]'),
      dialogOpen: view.dialogs.length > 0,
      running: machine.visual === 'run',
      design: mode === 'design',
    });
    if (!c) return;
    e.preventDefault();
    exec(c, e);
  }
</script>

<svelte:window onkeydowncapture={keydown} />

<a class="skip" href="#stage">{t('common-skip-canvas')}</a>
<TopBar />
<div class="main">
  {#if mode === 'design'}<ToolRail />{:else}<nav class="toolrail" aria-hidden="true"></nav>{/if}
  <Stage bind:this={stage} {mode} onzoom={(z) => (zoom = z)} />
  {#if env.layout === 'medium' && mode === 'design'}<DockRail />{/if}
  <Dock panels={{ machine: machinePanel, console: consolePanel, layers: layersPanel, object: objectPanel }} />
</div>
<StatusBar {zoom} />
{#if env.layout === 'narrow'}
  <BottomBar>
    <button
      class="btn primary big"
      disabled={machine.visual !== 'idle' || !machine.state?.program}
      onclick={() => laser({ action: 'job.run', params: {} })}
      ><Icon name="play" /><span>{t('machine-start')}</span></button
    >
    <button class="btn big" disabled={!progress(machine.state?.machine)} onclick={() => exec('pause')}
      ><Icon name="pause" /><span>{t('machine-pause')}</span></button
    >
    <button class="btn big" disabled={machine.visual !== 'idle'} onclick={() => exec('frame')}
      ><Icon name="frame" /><span>{t('machine-frame')}</span></button
    >
  </BottomBar>
{/if}
<Dialogs
  extra={{
    recover: recoverDialog,
    fire_confirmation: fireDialog,
    welcome: welcomeDialog,
    import: importDialog,
    raster: rasterDialog,
    material_test: materialTestDialog,
  }}
/>
<Toasts />
{#if automation.request}<PermissionDialog request={automation.request} />{/if}

{#snippet machinePanel()}
  <MachinePanel />
{/snippet}
{#snippet consolePanel()}
  <ConsolePanel />
{/snippet}
{#snippet recoverDialog(close: () => void)}
  <RecoverDialog onclose={close} />
{/snippet}
{#snippet layersPanel()}
  <LayersPanel {doc} />
{/snippet}
{#snippet objectPanel()}
  <ObjectPanel {doc} />
{/snippet}
{#snippet welcomeDialog(close: () => void)}
  <WelcomeDialog onclose={() => closeWelcome(close)} />
{/snippet}
{#snippet importDialog(close: () => void)}
  <ImportDialog onclose={close} />
{/snippet}
{#snippet rasterDialog(close: () => void)}
  <RasterDialog onclose={close} />
{/snippet}
{#snippet materialTestDialog(close: () => void)}
  <MaterialTestDialog onclose={close} />
{/snippet}
{#snippet fireDialog()}
  <FireConfirmation />
{/snippet}
