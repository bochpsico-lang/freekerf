// Number formatting for the UI (units, times). Locale-aware decimals come with i18n.

/** A length in mm shown in the user's units, 2 decimals (mm) or 3 (in). */
export function formatLength(mm: number, units: 'mm' | 'in'): string {
  return units === 'in' ? (mm / 25.4).toFixed(3) : mm.toFixed(2);
}

/** Milliseconds as `h:mm:ss` or `m:ss`. */
export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`;
}
