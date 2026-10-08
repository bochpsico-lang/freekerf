import { reportBench } from './ipc/bench';
import { TauriService } from './ipc/tauri';
import { start, startBench } from './start';

if (location.hash === '#bench-canvas') {
  void startBench().then(reportBench);
} else {
  start(new TauriService());
}
