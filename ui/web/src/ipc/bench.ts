// Sends the canvas benchmark report to the app, which prints it and exits
// (`freekerf-desktop --bench-canvas`).
import { invoke } from '@tauri-apps/api/core';

import type { BenchReport } from '../canvas/bench';

export function reportBench(report: BenchReport): Promise<void> {
  return invoke('bench_report', { report });
}
