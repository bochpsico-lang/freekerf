// @vitest-environment node
// The bundle check catches a leaked fake service.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { FAKE_SERVICE_MARKER } from '../../src/dev/fake-service';

const script = new URL('../../scripts/check-bundle.mjs', import.meta.url).pathname;

function check(files: Record<string, string>): boolean {
  const dir = mkdtempSync(join(tmpdir(), 'fk-bundle-'));
  for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, name), text);
  try {
    execFileSync('node', [script, dir], { stdio: 'pipe' });
    return true;
  } catch {
    return false;
  }
}

describe('check-bundle', () => {
  it('passes a clean bundle', () => {
    expect(check({ 'index.html': '<html></html>', 'app.js': 'console.log(1)' })).toBe(true);
  });

  it('fails when the fake service leaked', () => {
    expect(check({ 'app.js': `const m = "${FAKE_SERVICE_MARKER}";` })).toBe(false);
    expect(check({ 'dev.html': '<html></html>' })).toBe(false);
  });
});
