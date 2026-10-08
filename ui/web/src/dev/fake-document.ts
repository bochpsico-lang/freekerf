// A fake of the Design mode contract (doc/features/ui-contract.md) for development
// and component tests until M2 implements it in Rust (M4b). In memory: layers,
// objects (rectangles, ellipses, text boxes, imported SVG shapes) with a 2×3
// transform, selection, undo/redo, a tiny SVG importer and the migration scan.
// Never part of a production build (see fake-service.ts).

import type {
  AlignHow,
  Color,
  DocumentActionName,
  DocumentActions,
  DocumentService,
  DocumentSummary,
  ImportOptions,
  ImportReport,
  Layer,
  ObjectKind,
  ObjectSummary,
  TransformOp,
} from '../contract/document';
import { encodeToolpath, type SegmentRecord } from '../canvas/toolpath';
import { PALETTE_COLORS } from '../design/layers';
import { ActionFailure } from '../service';

type Affine = [number, number, number, number, number, number];
const IDENTITY: Affine = [1, 0, 0, 1, 0, 0];

interface FakeObject {
  id: number;
  layer: number;
  name: string;
  kind: ObjectKind;
  /** Polylines in object coordinates (mm, Y up). */
  paths: [number, number][][];
  transform: Affine;
  text?: { text: string; font: string; height_mm: number };
}

interface Doc {
  layers: Layer[];
  objects: FakeObject[];
  selection: number[];
  current_layer: number;
  next_id: number;
}

function apply(m: Affine, x: number, y: number): [number, number] {
  return [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
}

/** `a` after `b`. */
function compose(a: Affine, b: Affine): Affine {
  return [
    a[0] * b[0] + a[2] * b[1],
    a[1] * b[0] + a[3] * b[1],
    a[0] * b[2] + a[2] * b[3],
    a[1] * b[2] + a[3] * b[3],
    a[0] * b[4] + a[2] * b[5] + a[4],
    a[1] * b[4] + a[3] * b[5] + a[5],
  ];
}

function opMatrix(op: TransformOp): Affine {
  if ('translate' in op) return [1, 0, 0, 1, op.translate[0], op.translate[1]];
  const [px, py] = op.pivot;
  const to: Affine = [1, 0, 0, 1, px, py];
  const from: Affine = [1, 0, 0, 1, -px, -py];
  let m: Affine;
  if ('scale' in op) m = [op.scale[0], 0, 0, op.scale[1], 0, 0];
  else {
    const a = (op.rotate_deg * Math.PI) / 180;
    m = [Math.cos(a), Math.sin(a), -Math.sin(a), Math.cos(a), 0, 0];
  }
  return compose(to, compose(m, from));
}

function rectPath(x: number, y: number, w: number, h: number): [number, number][] {
  return [
    [x, y],
    [x + w, y],
    [x + w, y + h],
    [x, y + h],
    [x, y],
  ];
}

function ellipsePath(cx: number, cy: number, rx: number, ry: number): [number, number][] {
  const n = 64;
  return Array.from({ length: n + 1 }, (_, i) => {
    const a = (i / n) * Math.PI * 2;
    return [cx + rx * Math.cos(a), cy + ry * Math.sin(a)] as [number, number];
  });
}

/** A text stand-in: one stroke per character cell (the real one uses Hershey fonts). */
function textPaths(text: string, x: number, y: number, h: number): [number, number][][] {
  const w = h * 0.6;
  const out: [number, number][][] = [];
  [...text].forEach((ch, i) => {
    if (ch === ' ') return;
    const x0 = x + i * w;
    out.push([
      [x0, y],
      [x0 + w * 0.4, y + h],
      [x0 + w * 0.8, y],
    ]);
    out.push([
      [x0 + w * 0.2, y + h * 0.4],
      [x0 + w * 0.6, y + h * 0.4],
    ]);
  });
  return out.length ? out : [rectPath(x, y, w, h)];
}

function newLayer(id: number, color: Color): Layer {
  return {
    id,
    color,
    mode: 'line',
    speed_mm_min: 1000,
    power: 500,
    passes: 1,
    fill: { pattern: 'horizontal', lines_per_mm: 10 },
    visible: true,
  };
}

/** Parses the shapes of a simple SVG (rect, circle, ellipse, line, polyline, polygon). */
export function parseSvg(text: string): { color: Color; path: [number, number][] }[] {
  const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
  const out: { color: Color; path: [number, number][] }[] = [];
  const num = (el: Element, a: string) => Number(el.getAttribute(a) ?? 0);
  const height = Number(doc.documentElement.getAttribute('height')?.replace(/[a-z]+$/i, '') ?? 0);
  const flip = (p: [number, number][]) =>
    p.map(([x, y]) => [x, height ? height - y : -y] as [number, number]);
  for (const el of doc.querySelectorAll('rect, circle, ellipse, line, polyline, polygon')) {
    const color = (el.getAttribute('stroke') ?? el.getAttribute('fill') ?? '#000000').toUpperCase();
    let p: [number, number][] = [];
    switch (el.tagName) {
      case 'rect':
        p = rectPath(num(el, 'x'), num(el, 'y'), num(el, 'width'), num(el, 'height'));
        break;
      case 'circle':
        p = ellipsePath(num(el, 'cx'), num(el, 'cy'), num(el, 'r'), num(el, 'r'));
        break;
      case 'ellipse':
        p = ellipsePath(num(el, 'cx'), num(el, 'cy'), num(el, 'rx'), num(el, 'ry'));
        break;
      case 'line':
        p = [
          [num(el, 'x1'), num(el, 'y1')],
          [num(el, 'x2'), num(el, 'y2')],
        ];
        break;
      default: {
        const v = (el.getAttribute('points') ?? '')
          .trim()
          .split(/[\s,]+/)
          .map(Number);
        for (let i = 0; i + 1 < v.length; i += 2) p.push([v[i]!, v[i + 1]!]);
        if (el.tagName === 'polygon' && p.length) p.push(p[0]!);
      }
    }
    if (p.length > 1) out.push({ color: /^#[0-9A-F]{6}$/.test(color) ? color : '#000000', path: flip(p) });
  }
  return out;
}

export class FakeDocument implements DocumentService {
  private doc: Doc = {
    layers: [newLayer(1, PALETTE_COLORS[0]!)],
    objects: [],
    selection: [],
    current_layer: 1,
    next_id: 1,
  };
  private undoStack: string[] = [];
  private redoStack: string[] = [];
  private revision = 1;
  private geometryRevision = 1;
  private listeners = new Set<(revision: number) => void>();
  readonly calls: { action: string; params: unknown }[] = [];
  /** What `migrate.detect` finds (tests change it). */
  migration = { found: true, path: '/home/user/.wine/drive_c/users/user/AppData/Roaming/LaserGRBL' };

  subscribe(listener: (revision: number) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private changed(geometry: boolean): void {
    this.revision++;
    if (geometry) this.geometryRevision++;
    for (const l of this.listeners) l(this.revision);
  }

  private edit(geometry: boolean, f: (d: Doc) => void): null {
    this.undoStack.push(JSON.stringify(this.doc));
    this.redoStack = [];
    f(this.doc);
    this.changed(geometry);
    return null;
  }

  private obj(id: number): FakeObject {
    const o = this.doc.objects.find((x) => x.id === id);
    if (!o) throw new ActionFailure('document', { kind: 'invalid_params', message: `no object ${id}` });
    return o;
  }

  private add(
    kind: ObjectKind,
    name: string,
    paths: [number, number][][],
    text?: FakeObject['text'],
  ): number {
    const id = this.doc.next_id++;
    this.doc.objects.push({
      id,
      layer: this.doc.current_layer,
      name,
      kind,
      paths,
      transform: IDENTITY,
      text,
    });
    this.doc.selection = [id];
    return id;
  }

  private bounds(o: FakeObject): [number, number, number, number] | null {
    let b: [number, number, number, number] | null = null;
    for (const p of o.paths) {
      for (const [x, y] of p) {
        const [tx, ty] = apply(o.transform, x, y);
        b = b
          ? [Math.min(b[0], tx), Math.min(b[1], ty), Math.max(b[2], tx), Math.max(b[3], ty)]
          : [tx, ty, tx, ty];
      }
    }
    return b;
  }

  private summary(): DocumentSummary {
    const objects: ObjectSummary[] = this.doc.objects.map((o) => ({
      id: o.id,
      layer: o.layer,
      name: o.name,
      kind: o.kind,
      bounds: this.bounds(o),
      transform: o.transform,
      ...(o.text ? { text: o.text } : {}),
    }));
    return structuredClone({
      revision: this.revision,
      geometry_revision: this.geometryRevision,
      name: '',
      dirty: this.undoStack.length > 0,
      layers: this.doc.layers,
      objects,
      selection: this.doc.selection,
      current_layer: this.doc.current_layer,
      can_undo: this.undoStack.length > 0,
      can_redo: this.redoStack.length > 0,
    });
  }

  private layerForColor(color: Color): number {
    const c = color.toUpperCase();
    const found = this.doc.layers.find((l) => l.color.toUpperCase() === c);
    if (found) return found.id;
    const id = Math.max(0, ...this.doc.layers.map((l) => l.id)) + 1;
    this.doc.layers.push(newLayer(id, c));
    return id;
  }

  private handlers: {
    [A in DocumentActionName]: (p: DocumentActions[A]['params']) => DocumentActions[A]['result'];
  } = {
    'document.get': () => this.summary(),
    'document.select': (p) => {
      const cur = new Set(this.doc.selection);
      if (p.mode === 'replace') cur.clear();
      for (const id of p.ids) {
        if (p.mode === 'toggle' && cur.has(id)) cur.delete(id);
        else cur.add(id);
      }
      this.doc.selection = [...cur].filter((id) => this.doc.objects.some((o) => o.id === id));
      this.changed(false);
      return null;
    },
    'document.select_all': () => {
      this.doc.selection = this.doc.objects.map((o) => o.id);
      this.changed(false);
      return null;
    },
    'document.add_shape': (p) => {
      let id = 0;
      this.edit(true, () => {
        const path =
          p.shape === 'rect'
            ? rectPath(p.x, p.y, p.width, p.height)
            : ellipsePath(p.x + p.width / 2, p.y + p.height / 2, p.width / 2, p.height / 2);
        id = this.add('shape', p.shape, [path]);
      });
      return { id };
    },
    'document.add_text': (p) => {
      let id = 0;
      this.edit(true, () => {
        id = this.add('text', p.text, textPaths(p.text, p.x, p.y, p.height_mm), {
          text: p.text,
          font: p.font ?? 'hershey-simplex',
          height_mm: p.height_mm,
        });
      });
      return { id };
    },
    'document.set_text': (p) =>
      this.edit(true, () => {
        const o = this.obj(p.id);
        if (!o.text)
          throw new ActionFailure('document.set_text', { kind: 'invalid_params', message: 'not a text' });
        const [x, y] = o.paths[0]?.[0] ?? [0, 0];
        o.text = {
          ...o.text,
          ...(p.text !== undefined ? { text: p.text } : {}),
          ...(p.height_mm ? { height_mm: p.height_mm } : {}),
        };
        o.name = o.text.text;
        o.paths = textPaths(o.text.text, x, y, o.text.height_mm);
      }),
    'document.transform': (p) =>
      this.edit(true, () => {
        const m = opMatrix(p.op);
        for (const id of p.ids) {
          const o = this.obj(id);
          o.transform = compose(m, o.transform);
        }
      }),
    'document.align': (p) => this.edit(true, () => this.align(p.ids, p.how)),
    'document.delete': (p) =>
      this.edit(true, (d) => {
        d.objects = d.objects.filter((o) => !p.ids.includes(o.id));
        d.selection = d.selection.filter((id) => !p.ids.includes(id));
      }),
    'document.duplicate': (p) => {
      const ids: number[] = [];
      this.edit(true, (d) => {
        const [dx, dy] = p.offset_mm ?? [5, -5];
        for (const id of p.ids) {
          const o = structuredClone(this.obj(id));
          o.id = d.next_id++;
          o.transform = compose([1, 0, 0, 1, dx, dy], o.transform);
          d.objects.push(o);
          ids.push(o.id);
        }
        d.selection = ids;
      });
      return { ids };
    },
    'document.set_layer': (p) =>
      this.edit(true, () => {
        if (!this.doc.layers.some((l) => l.id === p.layer)) {
          throw new ActionFailure('document.set_layer', {
            kind: 'invalid_params',
            message: `no layer ${p.layer}`,
          });
        }
        for (const id of p.ids) this.obj(id).layer = p.layer;
      }),
    'document.layer_for_color': (p) => {
      let layer = 0;
      this.edit(false, () => {
        layer = this.layerForColor(p.color);
      });
      return { layer };
    },
    'document.set_current_layer': (p) => {
      this.doc.current_layer = p.layer;
      this.changed(false);
      return null;
    },
    'document.layer_update': (p) =>
      this.edit(false, (d) => {
        const l = d.layers.find((x) => x.id === p.layer);
        if (!l)
          throw new ActionFailure('document.layer_update', {
            kind: 'invalid_params',
            message: `no layer ${p.layer}`,
          });
        const patch: Partial<Layer> = { ...p };
        delete patch.id;
        delete (patch as { layer?: number }).layer;
        Object.assign(l, patch);
      }),
    'document.layer_move': (p) =>
      this.edit(false, (d) => {
        const i = d.layers.findIndex((x) => x.id === p.layer);
        if (i < 0) return;
        const [l] = d.layers.splice(i, 1);
        d.layers.splice(Math.max(0, Math.min(d.layers.length, p.to)), 0, l!);
      }),
    'document.undo': () => {
      const prev = this.undoStack.pop();
      if (prev) {
        this.redoStack.push(JSON.stringify(this.doc));
        this.doc = JSON.parse(prev) as Doc;
        this.changed(true);
      }
      return null;
    },
    'document.redo': () => {
      const next = this.redoStack.pop();
      if (next) {
        this.undoStack.push(JSON.stringify(this.doc));
        this.doc = JSON.parse(next) as Doc;
        this.changed(true);
      }
      return null;
    },
    'document.new': () =>
      this.edit(true, (d) => {
        d.objects = [];
        d.selection = [];
      }),
    'migrate.detect': () =>
      this.migration.found
        ? {
            found: true,
            path: this.migration.path,
            items: [
              { kind: 'connection', count: 1 },
              { kind: 'buttons', count: 14 },
              { kind: 'materials', count: 236 },
              { kind: 'counters', count: 2 },
            ],
          }
        : { found: false, path: null, items: [] },
    'migrate.run': (p) => ({
      imported: p.items.map((kind) => ({
        kind,
        count: kind === 'buttons' ? 14 : kind === 'materials' ? 236 : 1,
      })),
      warnings: p.items.includes('buttons')
        ? ['2 buttons use variables that changed syntax: review them']
        : [],
    }),
  };

  private align(ids: number[], how: AlignHow): void {
    const boxes = ids.map((id) => [id, this.bounds(this.obj(id))] as const).filter(([, b]) => b);
    if (!boxes.length) return;
    const all = boxes.reduce<[number, number, number, number]>(
      (a, [, b]) => [
        Math.min(a[0], b![0]),
        Math.min(a[1], b![1]),
        Math.max(a[2], b![2]),
        Math.max(a[3], b![3]),
      ],
      [Infinity, Infinity, -Infinity, -Infinity],
    );
    for (const [id, b] of boxes) {
      const [x0, y0, x1, y1] = b!;
      const d: Record<AlignHow, [number, number]> = {
        left: [all[0] - x0, 0],
        right: [all[2] - x1, 0],
        center_h: [(all[0] + all[2]) / 2 - (x0 + x1) / 2, 0],
        top: [0, all[3] - y1],
        bottom: [0, all[1] - y0],
        middle_v: [0, (all[1] + all[3]) / 2 - (y0 + y1) / 2],
      };
      const o = this.obj(id);
      o.transform = compose([1, 0, 0, 1, d[how][0], d[how][1]], o.transform);
    }
  }

  async dispatch<A extends DocumentActionName>(
    action: A,
    params: DocumentActions[A]['params'],
  ): Promise<DocumentActions[A]['result']> {
    this.calls.push({ action, params });
    const h = this.handlers[action] as (p: DocumentActions[A]['params']) => DocumentActions[A]['result'];
    return h(params);
  }

  async geometry(): Promise<ArrayBuffer> {
    const segs: SegmentRecord[] = [];
    this.doc.objects.forEach((o, index) => {
      const layer = this.doc.layers.findIndex((l) => l.id === o.layer);
      if (!this.doc.layers[layer]?.visible) return;
      for (const p of o.paths) {
        for (let i = 1; i < p.length; i++) {
          segs.push({
            from: apply(o.transform, p[i - 1]![0], p[i - 1]![1]),
            to: apply(o.transform, p[i]![0], p[i]![1]),
            power: layer,
            command: index,
          });
        }
      }
    });
    return encodeToolpath(segs, this.doc.objects.length);
  }

  async import(name: string, bytes: ArrayBuffer, options: ImportOptions): Promise<ImportReport> {
    this.calls.push({ action: 'import', params: { name, options } });
    const shapes = parseSvg(new TextDecoder().decode(bytes));
    const before = this.doc.layers.length;
    this.edit(true, (d) => {
      const ids: number[] = [];
      for (const s of shapes) {
        const layer = options.colors === 'unify' ? d.current_layer : this.layerForColor(s.color);
        const id = d.next_id++;
        d.objects.push({ id, layer, name, kind: 'shape', paths: [s.path], transform: IDENTITY });
        ids.push(id);
      }
      d.selection = ids;
    });
    return { objects: shapes.length, layers: this.doc.layers.length - before, unsupported: 0, warnings: [] };
  }
}
