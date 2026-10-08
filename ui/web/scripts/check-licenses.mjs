// npm license policy (same intent as deny.toml for Rust): every package in the
// dependency tree, development tools included, must use a license compatible with
// GPL-3.0-or-later. The list is an allow-list; anything else fails.
import { init } from 'license-checker-rseidelsohn';

const ALLOW = new Set([
  'MIT',
  'MIT-0',
  'ISC',
  'BSD-2-Clause',
  'BSD-3-Clause',
  '0BSD',
  'Apache-2.0',
  'Zlib',
  'CC0-1.0',
  'CC-BY-3.0', // data files (SPDX license lists of the license checker), not code
  'CC-BY-4.0', // data files (e.g. caniuse-lite), not code
  'Unlicense',
  'BlueOak-1.0.0',
  'Python-2.0', // argparse
  'MPL-2.0',
  'LGPL-3.0-or-later',
  'GPL-3.0-or-later',
]);

function ok(expr) {
  // SPDX expressions: `(A OR B)` passes if one side passes; `A AND B` needs both.
  const e = expr.replace(/[()]/g, '').trim();
  if (e.includes(' OR ')) return e.split(' OR ').some((x) => ok(x));
  if (e.includes(' AND ')) return e.split(' AND ').every((x) => ok(x));
  return ALLOW.has(e.replace(/\*$/, ''));
}

init({ start: new URL('..', import.meta.url).pathname, excludePrivatePackages: true }, (err, packages) => {
  if (err) {
    console.error(err);
    process.exit(2);
  }
  const bad = Object.entries(packages).filter(([, p]) => {
    const l = Array.isArray(p.licenses) ? p.licenses.join(' AND ') : String(p.licenses ?? 'UNKNOWN');
    return !ok(l);
  });
  for (const [name, p] of bad) console.error(`not allowed: ${name} (${p.licenses})`);
  if (bad.length) process.exit(1);
  console.log(`licenses ok: ${Object.keys(packages).length} packages`);
});
