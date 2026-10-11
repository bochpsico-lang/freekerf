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
   * Draws a rectangle or an ellipse on a layer (default: the current layer) and selects it. One undo step.
   *
   * Action `document.add_shape`, tier `edit`.
   */
  addShape(params: { height: number; layer?: number | null; shape: "rect" | "ellipse"; width: number; x: number; y: number }): unknown;
  /**
   * Writes text in the single-stroke font on a layer (default: the current layer) and selects it; the text stays editable (`document.set_text`). One undo step.
   *
   * Action `document.add_text`, tier `edit`.
   */
  addText(params: { height_mm?: number | null; layer?: number | null; text: string; x: number; y: number }): unknown;
  /**
   * Lines up objects (default: the selection, at least two) on the box around them: left, center_h, right, top, middle_v or bottom. One undo step.
   *
   * Action `document.align`, tier `edit`.
   */
  align(params: { how: "left" | "center_h" | "right" | "top" | "middle_v" | "bottom"; ids?: number[] | null }): unknown;
  /**
   * Copies objects (default: the selection) in a grid of `columns` × `rows` (the originals are the top-left cell; rows go down), `gap_mm` apart; selects the originals and the copies. One undo step.
   *
   * Action `document.array`, tier `edit`.
   */
  array(params: { columns: number; gap_mm?: number[] | null; ids?: number[] | null; rows: number }): unknown;
  /**
   * Combines shapes (default: the selection, at least two) into one: union (weld), difference (the first in drawing order minus the others), intersection or xor. The result takes the first one's layer and is selected. One undo step.
   *
   * Action `document.boolean`, tier `edit`.
   */
  boolean(params: { ids?: number[] | null; op: "union" | "difference" | "intersection" | "xor" }): unknown;
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
   * Copies objects (default: the selection), offset by `offset_mm`, and selects the copies. One undo step.
   *
   * Action `document.duplicate`, tier `edit`.
   */
  duplicate(params?: { ids?: number[] | null; offset_mm?: number[] | null }): unknown;
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
   * Imports a file given as base64 bytes (`data`), or as `text` for SVG and DXF, into the document: SVG and DXF by colour layer (or all on the current layer), images on a layer of their own. Returns the new objects and layers. One undo step.
   *
   * Action `document.import_data`, tier `edit`.
   */
  importData(params: { colors?: "keep" | "unify"; data?: string | null; dpi?: number | null; format: "svg" | "dxf" | "image"; name?: string; power?: number | null; speed_mm_min?: number | null; text?: string | null }): unknown;
  /**
   * The layer of a colour, created (line, 1000 mm/min, power 1000) when missing; it becomes the current layer. Creating it is one undo step.
   *
   * Action `document.layer_for_color`, tier `edit`.
   */
  layerForColor(params: { color: string }): unknown;
  /**
   * Draws a material test (design/ux.md flow 6): a grid of cells, power rising by row and speed by column, each cell on a layer of its own with that power and speed, labelled in single-stroke text on a layer of low power. One undo step.
   *
   * Action `document.material_test`, tier `edit`.
   */
  materialTest(params: { cell_mm?: number | null; gap_mm?: number | null; max_s?: number | null; mode?: "fill" | "line"; origin?: number[] | null; power_pct: { from: number; steps: number; to: number }; speed_mm_min: { from: number; steps: number; to: number } }): unknown;
  /**
   * Moves a layer in the layer list (the order of layers of the same mode is the order they run). One undo step.
   *
   * Action `document.move_layer`, tier `edit`.
   */
  moveLayer(params: { id: number; to: number }): unknown;
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
   * Adds the outline of shapes (default: the selection) grown or shrunk by `distance_mm`, as new objects on the same layers, selected. One undo step.
   *
   * Action `document.offset`, tier `edit`.
   */
  offset(params: { distance_mm: number; ids?: number[] | null; join?: "round" | "square" | "miter" }): unknown;
  /**
   * Opens a `.fkproj` project file (the undo history starts again).
   *
   * Action `document.open`, tier `edit`.
   */
  open(params: { path: string }): unknown;
  /**
   * The bitmap an image object's raster settings (or the ones given) make: grey levels, or black and white when dithered; a PNG, base64.
   *
   * Action `document.raster_preview`, tier `read`.
   */
  rasterPreview(params: { id: number; settings?: { adjustments: { blue: number; brightness: number; contrast: number; formula: "simple_average" | "weight_average" | "optical_correct" | "custom"; green: number; red: number; threshold?: number | null; white_clip: number }; border_speed_mm_min: number; centerline: { corner_threshold?: number | null; line_threshold?: number | null }; direction: "horizontal" | "vertical" | "diagonal"; dithering: "atkinson" | "floyd_steinberg" | "burks" | "jarvis" | "random" | "sierra2" | "sierra3" | "sierra_light" | "stucki"; interpolation: "nearest_neighbor" | "bilinear" | "bicubic" | "high_quality_bilinear" | "high_quality_bicubic"; laser_off: string; laser_on: string; lines_per_mm: number; mark_speed_mm_min: number; max_power: number; min_power: number; tool: "line_to_line" | "dithering" | "vectorize" | "centerline" | "passthrough"; vectorize: { adaptive_quality: boolean; down_sampling?: number | null; fill?: { direction: unknown; kind: unknown } | null; fill_lines_per_mm: number; optimize?: number | null; optimize_fast: boolean; smoothing?: number | null; spot_removal?: number | null } } | null }): unknown;
  /**
   * The raster dialog's settings of an image object (tool, dithering, resolution, grey adjustments, speeds, powers).
   *
   * Action `document.raster_settings`, tier `read`.
   */
  rasterSettings(params: { id: number }): unknown;
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
  select(params: { ids: number[]; mode?: "replace" | "add" | "remove" | "toggle" }): unknown;
  /**
   * Selects every object.
   *
   * Action `document.select_all`, tier `ui`.
   */
  selectAll(params?: Record<string, unknown>): unknown;
  /**
   * Chooses the layer new shapes and text go to.
   *
   * Action `document.set_current_layer`, tier `ui`.
   */
  setCurrentLayer(params: { id: number }): unknown;
  /**
   * Changes the raster dialog's settings of an image object. One undo step.
   *
   * Action `document.set_raster`, tier `edit`.
   */
  setRaster(params: { id: number; settings: { adjustments: { blue: number; brightness: number; contrast: number; formula: "simple_average" | "weight_average" | "optical_correct" | "custom"; green: number; red: number; threshold?: number | null; white_clip: number }; border_speed_mm_min: number; centerline: { corner_threshold?: number | null; line_threshold?: number | null }; direction: "horizontal" | "vertical" | "diagonal"; dithering: "atkinson" | "floyd_steinberg" | "burks" | "jarvis" | "random" | "sierra2" | "sierra3" | "sierra_light" | "stucki"; interpolation: "nearest_neighbor" | "bilinear" | "bicubic" | "high_quality_bilinear" | "high_quality_bicubic"; laser_off: string; laser_on: string; lines_per_mm: number; mark_speed_mm_min: number; max_power: number; min_power: number; tool: "line_to_line" | "dithering" | "vectorize" | "centerline" | "passthrough"; vectorize: { adaptive_quality: boolean; down_sampling?: number | null; fill?: { direction: unknown; kind: "raster" } | { direction: unknown; kind: "hatch" } | null; fill_lines_per_mm: number; optimize?: number | null; optimize_fast: boolean; smoothing?: number | null; spot_removal?: number | null } } }): unknown;
  /**
   * Changes a text object's text or font height (its position, rotation and scale stay). One undo step.
   *
   * Action `document.set_text`, tier `edit`.
   */
  setText(params: { height_mm?: number | null; id: number; text?: string | null }): unknown;
  /**
   * The document as `.fkproj` text.
   *
   * Action `document.to_fkproj`, tier `read`.
   */
  toFkproj(params?: Record<string, unknown>): unknown;
  /**
   * Moves, scales, mirrors or rotates objects (default: the selection): `{translate}`, `{scale, pivot}`, `{rotate_deg, pivot}` or `{matrix}`. Images only move and scale. One undo step.
   *
   * Action `document.transform`, tier `edit`.
   */
  transform(params: { ids?: number[] | null; op: { translate: number[] } | { pivot: number[]; scale: number[] } | { pivot: number[]; rotate_deg: number } | { matrix: number[] } }): unknown;
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

interface AppFire {
  /**
   * Cancels the pending laser request (the one given, or any). Always allowed.
   *
   * Action `fire.cancel`, tier `safety`.
   */
  cancel(params?: { id?: number | null }): unknown;
  /**
   * Asks to run a laser action: it opens the fire confirmation (checklist and countdown) and runs only when a person confirms on screen. Returns the pending request.
   *
   * Action `fire.request`, tier `machine`.
   */
  request(params: { action: string; params?: unknown }): unknown;
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
   * Runs the head around the bounds of the loaded program with the laser off (jog moves).
   *
   * Action `job.frame`, tier `machine`.
   */
  frame(params?: { feed_mm_min?: number | null }): unknown;
  /**
   * Plans the document (vectors, then images; centerline uses the `autotrace` program) and loads it as the job, placed by `start` (absolute coordinates, the head's current position, or the user origin 0,0) and `anchor` (which point of the job's box goes there).
   *
   * Action `job.from_document`, tier `edit`.
   */
  fromDocument(params?: { anchor?: "bottom_left" | "bottom" | "bottom_right" | "left" | "center" | "right" | "top_left" | "top" | "top_right"; start?: "absolute" | "current_position" | "user_origin" }): unknown;
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
   * Work area, laser mode, highest power and laser module counters, from the board settings.
   *
   * Action `machine.info`, tier `read`.
   */
  info(params?: Record<string, unknown>): unknown;
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
   * Sends a `$` command (`$$`, `$I`, `$H`, `$X`, `$N=V`, `$J=...`; not during a job). It cannot turn the laser on.
   *
   * Action `machine.system_command`, tier `machine`.
   */
  systemCommand(params: { line: string }): unknown;
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

interface AppMigrate {
  /**
   * What can be imported from LaserGRBL's data folder (`path`, default: LaserGRBL's folder on this computer): connection, preferences, laser counters, custom buttons, materials. Nothing is written.
   *
   * Action `migrate.detect`, tier `read`.
   */
  detect(params?: { path?: string | null }): unknown;
  /**
   * Imports the chosen items from LaserGRBL's data folder into FreeKerf's settings, machine cache and macro folder. LaserGRBL's files are only read.
   *
   * Action `migrate.run`, tier `edit`.
   */
  run(params: { items: ("connection" | "preferences" | "counters" | "buttons" | "materials")[]; path?: string | null }): unknown;
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
   * Closes a dialog (default: the top one). Closing the fire confirmation drops its laser request.
   *
   * Action `view.close_dialog`, tier `ui`.
   */
  closeDialog(params?: { name?: "settings" | "shortcuts" | "fire_confirmation" | "recover" | "welcome" | "import" | "raster" | "material_test" | "about" | null }): unknown;
  /**
   * Closes the drawer when it shows this panel.
   *
   * Action `view.close_panel`, tier `ui`.
   */
  closePanel(params: { name: "layers" | "object" | "machine" | "console" }): unknown;
  /**
   * Removes a toast.
   *
   * Action `view.dismiss_toast`, tier `ui`.
   */
  dismissToast(params: { id: number }): unknown;
  /**
   * Fits the work area in the canvas.
   *
   * Action `view.fit`, tier `ui`.
   */
  fit(params?: Record<string, unknown>): unknown;
  /**
   * Records which element has the keyboard focus.
   *
   * Action `view.focus`, tier `ui`.
   */
  focus(params?: { target?: string | null }): unknown;
  /**
   * The view: mode, tool, panel, dialogs, toasts, viewport, focus, the pending laser request.
   *
   * Action `view.get`, tier `read`.
   */
  get(params?: Record<string, unknown>): unknown;
  /**
   * Opens a dialog on top of the others (dialogs are not modal: STOP stays reachable).
   *
   * Action `view.open_dialog`, tier `ui`.
   */
  openDialog(params: { name: "settings" | "shortcuts" | "fire_confirmation" | "recover" | "welcome" | "import" | "raster" | "material_test" | "about" }): unknown;
  /**
   * Shows a panel of the dock (`layers`, `object`, `machine`, `console`; opens the drawer in the medium layout).
   *
   * Action `view.open_panel`, tier `ui`.
   */
  openPanel(params: { name: "layers" | "object" | "machine" | "console" }): unknown;
  /**
   * Switches between design and produce (produce shows the machine panel).
   *
   * Action `view.set_mode`, tier `ui`.
   */
  setMode(params: { mode: "design" | "produce" }): unknown;
  /**
   * Chooses the Design mode tool (select, rectangle, ellipse, text).
   *
   * Action `view.set_tool`, tier `ui`.
   */
  setTool(params: { tool: "select" | "rect" | "ellipse" | "text" }): unknown;
  /**
   * Centres the view on a point (mm) at a zoom (CSS pixels per mm).
   *
   * Action `view.set_viewport`, tier `ui`.
   */
  setViewport(params: { center: number[]; zoom: number }): unknown;
  /**
   * Shows a short message (at most 3 at once); returns its id.
   *
   * Action `view.toast`, tier `ui`.
   */
  toast(params: { args?: Record<string, unknown>; level?: "info" | "ok" | "warning" | "error"; message: string }): unknown;
}

/** The application: one namespace per action prefix. */
declare const app: {
  readonly console: AppConsole;
  readonly document: AppDocument;
  readonly fire: AppFire;
  readonly history: AppHistory;
  readonly job: AppJob;
  readonly machine: AppMachine;
  readonly migrate: AppMigrate;
  readonly session: AppSession;
  readonly settings: AppSettings;
  readonly state: AppState;
  readonly view: AppView;
};

/** Writes a line to the run's log (the app console). */
declare const console: { log(...values: unknown[]): void };
