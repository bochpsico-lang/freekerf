// Fails when the production bundle (dist/) contains the fake service of src/dev/
// or a development-only entry. Run after `npm run build` (CI does).
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const MARKER = 'freekerf-fake-service-do-not-ship';
const dist = process.argv[2] ?? fileURLToPath(new URL('../dist/', import.meta.url));

function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

const all = files(dist);
const leaks = all.filter((f) => /\.(js|html|css|map)$/.test(f) && readFileSync(f, 'utf8').includes(MARKER));
const devEntries = all.filter((f) => f.endsWith('dev.html'));
if (leaks.length || devEntries.length) {
  console.error('the fake service reached the production bundle:', [...leaks, ...devEntries].join(', '));
  process.exit(1);
}
console.log(`bundle ok: ${all.length} files, no fake service`);
