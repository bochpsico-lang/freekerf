// The Design mode contract (doc/features/ui-contract.md): what the UI needs from the
// M2 crates. Hand-written ONLY until M2 implements it in Rust (M4b); then this file
// is deleted and the same names come from the generated ../gen/api.ts (ADR 0013).
// Implemented today by the fake of ../dev/fake-document.ts.

/** `#RRGGBB`. */
export type Color = string;

export type LayerMode = 'line' | 'fill' | 'fill_and_line' | 'cut' | 'ignore';

/** freekerf-document `Layer`. */
export interface Layer {
  id: number;
  color: Color;
  name?: string;
  mode: LayerMode;
  speed_mm_min: number;
  power: number;
  passes: number;
  fill: { pattern: string; lines_per_mm: number };
  visible: boolean;
}

export type ObjectKind = 'shape' | 'image' | 'text' | 'unsupported';

export interface ObjectSummary {
  id: number;
  layer: number;
  name: string;
  kind: ObjectKind;
  /** `[xmin, ymin, xmax, ymax]` mm. */
  bounds: [number, number, number, number] | null;
  /** 2×3 affine `[a, b, c, d, e, f]`: x' = a x + c y + e, y' = b x + d y + f. */
  transform: [number, number, number, number, number, number];
  text?: { text: string; font: string; height_mm: number };
  image?: { width_px: number; height_px: number; dither: string };
}

export interface DocumentSummary {
  revision: number;
  geometry_revision: number;
  name: string;
  dirty: boolean;
  layers: Layer[];
  objects: ObjectSummary[];
  selection: number[];
  current_layer: number;
  can_undo: boolean;
  can_redo: boolean;
}

export type TransformOp =
  | { translate: [number, number] }
  | { scale: [number, number]; pivot: [number, number] }
  | { rotate_deg: number; pivot: [number, number] };

export type AlignHow = 'left' | 'center_h' | 'right' | 'top' | 'middle_v' | 'bottom';

export interface ImportOptions {
  colors: 'keep' | 'unify';
}

export interface ImportReport {
  objects: number;
  layers: number;
  unsupported: number;
  warnings: string[];
}

export type MigrationItemKind =
  'connection' | 'buttons' | 'materials' | 'counters' | 'shortcuts' | 'preferences' | 'language';

export interface MigrationScan {
  found: boolean;
  path: string | null;
  items: { kind: MigrationItemKind; count: number }[];
}

export interface MigrationReport {
  imported: { kind: MigrationItemKind; count: number }[];
  warnings: string[];
}

/** The proposed document actions: parameters and results. */
export interface DocumentActions {
  'document.get': { params: Record<string, never>; result: DocumentSummary };
  'document.select': { params: { ids: number[]; mode: 'replace' | 'add' | 'toggle' }; result: null };
  'document.select_all': { params: Record<string, never>; result: null };
  'document.add_shape': {
    params: { shape: 'rect' | 'ellipse'; x: number; y: number; width: number; height: number };
    result: { id: number };
  };
  'document.add_text': {
    params: { text: string; x: number; y: number; height_mm: number; font?: string };
    result: { id: number };
  };
  'document.set_text': {
    params: { id: number; text?: string; height_mm?: number; font?: string };
    result: null;
  };
  'document.transform': { params: { ids: number[]; op: TransformOp }; result: null };
  'document.align': { params: { ids: number[]; how: AlignHow }; result: null };
  'document.delete': { params: { ids: number[] }; result: null };
  'document.duplicate': {
    params: { ids: number[]; offset_mm?: [number, number] };
    result: { ids: number[] };
  };
  'document.set_layer': { params: { ids: number[]; layer: number }; result: null };
  'document.layer_for_color': { params: { color: Color }; result: { layer: number } };
  'document.set_current_layer': { params: { layer: number }; result: null };
  'document.layer_update': {
    params: { layer: number } & Partial<
      Pick<Layer, 'name' | 'mode' | 'speed_mm_min' | 'power' | 'passes' | 'fill' | 'visible'>
    >;
    result: null;
  };
  'document.layer_move': { params: { layer: number; to: number }; result: null };
  'document.undo': { params: Record<string, never>; result: null };
  'document.redo': { params: Record<string, never>; result: null };
  'document.new': { params: Record<string, never>; result: null };
  'migrate.detect': { params: Record<string, never>; result: MigrationScan };
  'migrate.run': { params: { items: MigrationItemKind[] }; result: MigrationReport };
}

export type DocumentActionName = keyof DocumentActions;

/** What the Design mode calls (M4b: implemented over dispatch and raw commands). */
export interface DocumentService {
  dispatch<A extends DocumentActionName>(
    action: A,
    params: DocumentActions[A]['params'],
  ): Promise<DocumentActions[A]['result']>;
  /** Drawable segments ("FKTP" v1: power = layer index, command = object index). */
  geometry(): Promise<ArrayBuffer>;
  /** Imports a file (raw bytes). */
  import(name: string, bytes: ArrayBuffer, options: ImportOptions): Promise<ImportReport>;
  /** `{"event": "document", "revision": n}`; returns the unsubscribe function. */
  subscribe(listener: (revision: number) => void): () => void;
}
