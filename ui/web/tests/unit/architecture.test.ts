// @vitest-environment node
// The architecture rules of eslint.config.js really fail on violations.
import { ESLint } from 'eslint';
import { describe, expect, it } from 'vitest';

const eslint = new ESLint({ cwd: process.cwd() });

async function errors(code: string, filePath: string): Promise<string[]> {
  const [result] = await eslint.lintText(code, { filePath });
  return (result?.messages ?? []).filter((m) => m.severity === 2).map((m) => m.ruleId ?? m.message);
}

describe('architecture lint', () => {
  it('only src/ipc talks to Tauri', async () => {
    const code = "import { invoke } from '@tauri-apps/api/core';\nvoid invoke('dispatch');\n";
    expect(await errors(code, 'src/lib/Thing.ts')).toContain('no-restricted-imports');
    expect(await errors(code, 'src/ipc/other.ts')).toEqual([]);
  });

  it('the fake service stays out of the app', async () => {
    const code = "import { FakeService } from '../dev/fake-service';\nexport const s = new FakeService();\n";
    expect(await errors(code, 'src/lib/Thing.ts')).toContain('no-restricted-imports');
    expect(await errors(code, 'src/ipc/x.ts')).toContain('no-restricted-imports');
    expect(await errors(code, 'src/lib/Thing.test.ts')).toEqual([]);
  });
});
