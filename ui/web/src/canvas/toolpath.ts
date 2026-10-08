// Decoder of the tool path buffer sent by the app (`freekerf_service::preview`,
// format "FKTP" v1). The segment block is handed to the GPU as is (no copy).

import type { Rect } from './camera';

export const TOOLPATH_MAGIC = 0x50544b46; // "FKTP" little endian
export const TOOLPATH_VERSION = 1;
export const HEADER_BYTES = 40;
/** Bytes per segment: f32 x0, y0, x1, y1, power; u32 command. */
export const SEGMENT_BYTES = 24;

/** A decoded tool path. */
export interface Toolpath {
  segmentCount: number;
  commandCount: number;
  /** Bounds of all segments (mm). */
  bounds: Rect;
  /** Highest S of the program (0 when none). */
  maxPower: number;
  /** The segment records (`segmentCount * SEGMENT_BYTES` bytes). */
  segments: DataView;
}

/** Error for a buffer that is not a tool path. */
export class ToolpathFormatError extends Error {
  override name = 'ToolpathFormatError';
}

/** Reads the buffer (throws {@link ToolpathFormatError} when malformed). */
export function decodeToolpath(buffer: ArrayBuffer): Toolpath {
  if (buffer.byteLength < HEADER_BYTES) throw new ToolpathFormatError('tool path: too short');
  const h = new DataView(buffer, 0, HEADER_BYTES);
  if (h.getUint32(0, true) !== TOOLPATH_MAGIC) throw new ToolpathFormatError('tool path: bad magic');
  const version = h.getUint32(4, true);
  if (version !== TOOLPATH_VERSION) throw new ToolpathFormatError(`tool path: version ${version}`);
  const segmentCount = h.getUint32(8, true);
  const needed = HEADER_BYTES + segmentCount * SEGMENT_BYTES;
  if (buffer.byteLength < needed) throw new ToolpathFormatError('tool path: truncated');
  const [x0, y0, x1, y1] = [16, 20, 24, 28].map((o) => h.getFloat32(o, true)) as [
    number,
    number,
    number,
    number,
  ];
  return {
    segmentCount,
    commandCount: h.getUint32(12, true),
    bounds: { x: x0, y: y0, width: x1 - x0, height: y1 - y0 },
    maxPower: h.getFloat32(32, true),
    segments: new DataView(buffer, HEADER_BYTES, segmentCount * SEGMENT_BYTES),
  };
}

/** One segment for tests and tools (the renderer reads the bytes directly). */
export interface SegmentRecord {
  from: [number, number];
  to: [number, number];
  power: number;
  command: number;
}

/** Builds a tool path buffer (tests, the fake service, the benchmark). */
export function encodeToolpath(segments: SegmentRecord[], commandCount: number): ArrayBuffer {
  const buf = new ArrayBuffer(HEADER_BYTES + segments.length * SEGMENT_BYTES);
  const v = new DataView(buf);
  let b = [Infinity, Infinity, -Infinity, -Infinity];
  let maxPower = 0;
  segments.forEach((s, i) => {
    const o = HEADER_BYTES + i * SEGMENT_BYTES;
    [s.from[0], s.from[1], s.to[0], s.to[1], s.power].forEach((x, k) => v.setFloat32(o + 4 * k, x, true));
    v.setUint32(o + 20, s.command, true);
    for (const [x, y] of [s.from, s.to]) {
      b = [Math.min(b[0]!, x), Math.min(b[1]!, y), Math.max(b[2]!, x), Math.max(b[3]!, y)];
    }
    maxPower = Math.max(maxPower, s.power);
  });
  if (!segments.length) b = [0, 0, 0, 0];
  v.setUint32(0, TOOLPATH_MAGIC, true);
  v.setUint32(4, TOOLPATH_VERSION, true);
  v.setUint32(8, segments.length, true);
  v.setUint32(12, commandCount, true);
  b.forEach((x, k) => v.setFloat32(16 + 4 * k, x, true));
  v.setFloat32(32, maxPower, true);
  return buf;
}

/** Reads segment `i` (tests and hit-testing). */
export function segmentAt(t: Toolpath, i: number): SegmentRecord {
  const o = i * SEGMENT_BYTES;
  const f = (k: number) => t.segments.getFloat32(o + 4 * k, true);
  return { from: [f(0), f(1)], to: [f(2), f(3)], power: f(4), command: t.segments.getUint32(o + 20, true) };
}

/** Head frame from the app (`freekerf_service::preview::head_frame`, 24 bytes). */
export interface HeadFrame {
  x: number;
  y: number;
  power: number;
  executed: number;
  target: number;
  running: boolean;
  connected: boolean;
}

/** Reads a head frame. */
export function decodeHead(buffer: ArrayBuffer): HeadFrame {
  if (buffer.byteLength < 24) throw new ToolpathFormatError('head frame: too short');
  const v = new DataView(buffer);
  const flags = v.getUint32(20, true);
  return {
    x: v.getFloat32(0, true),
    y: v.getFloat32(4, true),
    power: v.getFloat32(8, true),
    executed: v.getUint32(12, true),
    target: v.getUint32(16, true),
    running: (flags & 1) !== 0,
    connected: (flags & 2) !== 0,
  };
}
