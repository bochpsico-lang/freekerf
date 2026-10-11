// FreeKerf script API. Generated from the action registry by `cargo xtask gen-docs`. Do not edit.
// Every method dispatches one action; the caller's tier is checked on every call.

interface AppConsole {
  /**
   * Console rows (sent commands, answers, messages).
   *
   * Action `console.get`, tier `read`.
   */
  get(params?: { last?: number | null }): unknown;
}

interface AppDocument {
  /**
   * Adds a layer of a colour (line, 1000 mm/min, power 1000, 1 pass unless given). One undo step.
   *
   * Action `document.add_layer`, tier `edit`.
   */
  addLayer(params: { color: string; fill_pattern?: "horizontal" | "vertical" | "diagonal" | "reverse_diagonal" | "grid" | "diagonal_grid" | "cross" | "diagonal_cross" | "squares" | "zig_zag" | "hilbert" | "inset" | null; lines_per_mm?: number | null; mode?: "line" | "fill" | "fill_and_line" | "cut" | "ignore" | null; name?: string | null; passes?: number | null; power?: number | null; speed_mm_min?: number | null; visible?: boolean | null }): unknown;
  /**
   * Selects nothing.
   *
   * Action `document.clear_selection`, tier `ui`.
   */
  clearSelection(params?: Record<string, unknown>): unknown;
  /**
   * Deletes objects (default: the selection). One undo step.
   *
   * Action `document.delete`, tier `edit`.
   */
  delete(params?: { ids?: number[] | null }): unknown;
  /**
   * Replaces the document with a `.fkproj` text (the undo history starts again).
   *
   * Action `document.from_fkproj`, tier `edit`.
   */
  fromFkproj(params: { text: string }): unknown;
  /**
   * The whole document (layers, objects with geometry and images), as in a `.fkproj` file.
   *
   * Action `document.get`, tier `read`.
   */
  get(params?: Record<string, unknown>): unknown;
  /**
   * Imports an SVG, DXF or image file into the document (format from the extension). One undo step.
   *
   * Action `document.import`, tier `edit`.
   */
  import(params: { path: string }): unknown;
  /**
   * Imports a file given as base64 bytes (`data`), or as `text` for SVG and DXF, into the document: SVG and DXF by colour layer, images on a layer of their own. One undo step.
   *
   * Action `document.import_data`, tier `edit`.
   */
  importData(params: { data?: string | null; dpi?: number | null; format: "svg" | "dxf" | "image"; name?: string; power?: number | null; speed_mm_min?: number | null; text?: string | null }): unknown;
  /**
   * Moves objects (default: the selection) to another layer. One undo step.
   *
   * Action `document.move_to_layer`, tier `edit`.
   */
  moveToLayer(params: { ids?: number[] | null; layer: number }): unknown;
  /**
   * Starts an empty document (the undo history starts again).
   *
   * Action `document.new`, tier `edit`.
   */
  "new"(params?: Record<string, unknown>): unknown;
  /**
   * Opens a `.fkproj` project file (the undo history starts again).
   *
   * Action `document.open`, tier `edit`.
   */
  open(params: { path: string }): unknown;
  /**
   * Removes a layer and its objects. One undo step.
   *
   * Action `document.remove_layer`, tier `edit`.
   */
  removeLayer(params: { id: number }): unknown;
  /**
   * Saves the document as a `.fkproj` project file (written atomically).
   *
   * Action `document.save`, tier `edit`.
   */
  save(params: { path: string }): unknown;
  /**
   * Selects objects (replace, add to or remove from the selection). Unknown ids are ignored.
   *
   * Action `document.select`, tier `ui`.
   */
  select(params: { ids: number[]; mode?: "replace" | "add" | "remove" }): unknown;
  /**
   * Selects every object.
   *
   * Action `document.select_all`, tier `ui`.
   */
  selectAll(params?: Record<string, unknown>): unknown;
  /**
   * The document as `.fkproj` text.
   *
   * Action `document.to_fkproj`, tier `read`.
   */
  toFkproj(params?: Record<string, unknown>): unknown;
  /**
   * Moves objects (default: the selection) by `dx`, `dy` mm. One undo step.
   *
   * Action `document.translate`, tier `edit`.
   */
  translate(params: { dx: number; dy: number; ids?: number[] | null }): unknown;
  /**
   * Changes a layer's name, mode, speed, power, passes, visibility or filling. One undo step.
   *
   * Action `document.update_layer`, tier `edit`.
   */
  updateLayer(params: { fill_pattern?: "horizontal" | "vertical" | "diagonal" | "reverse_diagonal" | "grid" | "diagonal_grid" | "cross" | "diagonal_cross" | "squares" | "zig_zag" | "hilbert" | "inset" | null; id: number; lines_per_mm?: number | null; mode?: "line" | "fill" | "fill_and_line" | "cut" | "ignore" | null; name?: string | null; passes?: number | null; power?: number | null; speed_mm_min?: number | null; visible?: boolean | null }): unknown;
}

interface AppHistory {
  /**
   * Redoes the last undone change.
   *
   * Action `history.redo`, tier `edit`.
   */
  redo(params?: Record<string, unknown>): unknown;
  /**
   * Undoes the last document change.
   *
   * Action `history.undo`, tier `edit`.
   */
  undo(params?: Record<string, unknown>): unknown;
}

interface AppJob {
  /**
   * Stops streaming the job and sends `M5` (lines already in the board still run; use machine.stop to stop at once). Always allowed.
   *
   * Action `job.abort`, tier `safety`.
   */
  abort(params?: Record<string, unknown>): unknown;
  /**
   * Plans the document (vectors, then images; centerline uses the `autotrace` program) and loads it as the job.
   *
   * Action `job.from_document`, tier `edit`.
   */
  fromDocument(params?: Record<string, unknown>): unknown;
  /**
   * The loaded program (name, lines, estimated time, range).
   *
   * Action `job.info`, tier `read`.
   */
  info(params?: Record<string, unknown>): unknown;
  /**
   * Loads and analyzes a G-code file.
   *
   * Action `job.load`, tier `edit`.
   */
  load(params: { path: string }): unknown;
  /**
   * Loads and analyzes G-code given as text.
   *
   * Action `job.load_text`, tier `edit`.
   */
  loadText(params: { gcode: string; name: string }): unknown;
  /**
   * Runs the loaded program from a position, rebuilding position, power and modes first. Fires the laser.
   *
   * Action `job.resume`, tier `laser`.
   */
  resume(params: { homing?: boolean; position: number; set_wco?: boolean }): unknown;
  /**
   * Runs the loaded program from the start (header, program, footer, passes). Fires the laser.
   *
   * Action `job.run`, tier `laser`.
   */
  run(params?: { homing?: boolean; passes?: number | null }): unknown;
}

interface AppMachine {
  /**
   * Connects to the machine on a serial port, or to the built-in emulator.
   *
   * Action `machine.connect`, tier `machine`.
   */
  connect(params?: { baud?: number | null; emulator?: boolean; port?: string | null }): unknown;
  /**
   * Runs custom code (buttons, macros): `[expr]` evaluated, immediate characters sent at once.
   *
   * Action `machine.custom_code`, tier `laser`.
   */
  customCode(params: { code: string }): unknown;
  /**
   * Closes the connection.
   *
   * Action `machine.disconnect`, tier `machine`.
   */
  disconnect(params?: Record<string, unknown>): unknown;
  /**
   * Feed hold (pause the motion). Always allowed.
   *
   * Action `machine.feed_hold`, tier `safety`.
   */
  feedHold(params?: Record<string, unknown>): unknown;
  /**
   * Runs the homing cycle (`$H`).
   *
   * Action `machine.home`, tier `machine`.
   */
  home(params?: Record<string, unknown>): unknown;
  /**
   * Jogs one step in a direction (laser off).
   *
   * Action `machine.jog`, tier `machine`.
   */
  jog(params: { direction: string; feed_mm_min?: number | null; step_mm?: number | null }): unknown;
  /**
   * Stops a continuous jog. Always allowed.
   *
   * Action `machine.jog_stop`, tier `safety`.
   */
  jogStop(params?: Record<string, unknown>): unknown;
  /**
   * Jogs to a work position (laser off).
   *
   * Action `machine.jog_to`, tier `machine`.
   */
  jogTo(params: { feed_mm_min?: number | null; x: number; y: number }): unknown;
  /**
   * Serial ports found on this computer.
   *
   * Action `machine.list_ports`, tier `read`.
   */
  listPorts(params?: Record<string, unknown>): unknown;
  /**
   * Sets the override targets; the host steps the board towards them.
   *
   * Action `machine.overrides`, tier `machine`.
   */
  overrides(params: { feed: number; power: number; rapid: number }): unknown;
  /**
   * Reads the board settings again (`$$`).
   *
   * Action `machine.read_settings`, tier `machine`.
   */
  readSettings(params?: Record<string, unknown>): unknown;
  /**
   * Resumes after a feed hold or a door (cycle start).
   *
   * Action `machine.resume`, tier `machine`.
   */
  resume(params?: Record<string, unknown>): unknown;
  /**
   * Safety door (real-time 0x84): the board holds as if its door opened; resume with machine.resume. Always allowed.
   *
   * Action `machine.safety_door`, tier `safety`.
   */
  safetyDoor(params?: Record<string, unknown>): unknown;
  /**
   * Selects the laser module in use (life counters).
   *
   * Action `machine.select_laser`, tier `edit`.
   */
  selectLaser(params: { id: string }): unknown;
  /**
   * Sends one line of G-code or a `$` command (not during a job). It can turn the laser on.
   *
   * Action `machine.send`, tier `laser`.
   */
  send(params: { line: string }): unknown;
  /**
   * Sets the current position as work zero (`G92 X0 Y0 Z0`).
   *
   * Action `machine.set_zero`, tier `machine`.
   */
  setZero(params?: Record<string, unknown>): unknown;
  /**
   * STOP: cancels running scripts, then soft resets the board (laser off, queues flushed). Always allowed.
   *
   * Action `machine.stop`, tier `safety`.
   */
  stop(params?: Record<string, unknown>): unknown;
  /**
   * Unlocks the alarm state (`$X`).
   *
   * Action `machine.unlock`, tier `machine`.
   */
  unlock(params?: Record<string, unknown>): unknown;
  /**
   * Writes board settings (`$N=V`), one at a time; returns when the board answered all of them.
   *
   * Action `machine.write_settings`, tier `machine`.
   */
  writeSettings(params: { settings: Record<string, unknown> }): unknown;
}

interface AppSession {
  /**
   * The session as front ends and agents read it: revision, selection, view, undo/redo labels, layers and objects without geometry.
   *
   * Action `session.snapshot`, tier `read`.
   */
  snapshot(params?: Record<string, unknown>): unknown;
}

interface AppSettings {
  /**
   * Reads the settings (all, or one dotted key).
   *
   * Action `settings.get`, tier `read`.
   */
  get(params?: { key?: string | null }): unknown;
  /**
   * Changes one setting and saves the file. Automation settings are user-only and refused here.
   *
   * Action `settings.set`, tier `edit`.
   */
  set(params: { key: string; value: unknown }): unknown;
}

interface AppState {
  /**
   * Machine state, loaded program, issues and resume offer.
   *
   * Action `state.get`, tier `read`.
   */
  get(params?: Record<string, unknown>): unknown;
}

interface AppView {
  /**
   * Closes a dialog (default: the top one).
   *
   * Action `view.close_dialog`, tier `ui`.
   */
  closeDialog(params?: { name?: string | null }): unknown;
  /**
   * Closes a panel by name.
   *
   * Action `view.close_panel`, tier `ui`.
   */
  closePanel(params: { name: string }): unknown;
  /**
   * The view: mode, open panels, viewport, dialogs.
   *
   * Action `view.get`, tier `read`.
   */
  get(params?: Record<string, unknown>): unknown;
  /**
   * Opens a dialog by name (on top of the others).
   *
   * Action `view.open_dialog`, tier `ui`.
   */
  openDialog(params: { name: string }): unknown;
  /**
   * Opens a panel by name (`layers`, `console`, `jog`...).
   *
   * Action `view.open_panel`, tier `ui`.
   */
  openPanel(params: { name: string }): unknown;
  /**
   * Switches between design and produce.
   *
   * Action `view.set_mode`, tier `ui`.
   */
  setMode(params: { mode: "design" | "produce" }): unknown;
  /**
   * Centres the view on a point (mm) at a zoom (pixels per mm).
   *
   * Action `view.set_viewport`, tier `ui`.
   */
  setViewport(params: { center: number[]; zoom: number }): unknown;
}

/** The application: one namespace per action prefix. */
declare const app: {
  readonly console: AppConsole;
  readonly document: AppDocument;
  readonly history: AppHistory;
  readonly job: AppJob;
  readonly machine: AppMachine;
  readonly session: AppSession;
  readonly settings: AppSettings;
  readonly state: AppState;
  readonly view: AppView;
};

/** Writes a line to the run's log (the app console). */
declare const console: { log(...values: unknown[]): void };
