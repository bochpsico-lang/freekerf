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

describe('dispatch-only lint', () => {
  it('reports writes to projections of app state outside src/state', async () => {
    const svelte = (code: string) => `<script lang="ts">\n${code}\n</script>\n`;
    for (const code of [
      'let { machine }: { machine: { state: { x: number } } } = $props();\nmachine.state.x = 1;',
      'let { view }: { view: { mode: string } } = $props();\nview.mode = "design";',
      'let { ctx }: { ctx: { settings: { ui: { theme: string } } } } = $props();\nctx.settings.ui.theme = "light";',
      'let { preview }: { preview: { n: number } } = $props();\npreview.n++;',
      'let { view }: { view: { dialogs: string[] } } = $props();\ndelete view.dialogs;',
    ]) {
      expect(await errors(svelte(code), 'src/shell/TopBar.svelte'), code).toContain(
        'freekerf/no-app-state-mutation',
      );
    }
  });

  it('allows local state and the projections themselves', async () => {
    const ok =
      'let local = $state(0);\nexport function bump() { local += 1; }\nexport const el = { style: { width: String(local) } };\nel.style.width = "1px";\n';
    expect(await errors(`<script lang="ts">\n${ok}\n</script>\n`, 'src/shell/StatusBar.svelte')).toEqual([]);
    const inside = 'export class X { view = { mode: "a" }; set() { this.view.mode = "b"; } }\n';
    expect(await errors(inside, 'src/state/view.svelte.ts')).toEqual([]);
  });

  it('forbids browser storage for app state', async () => {
    expect(await errors('localStorage.setItem("theme", "dark");\n', 'src/shell/x.ts')).toContain(
      'no-restricted-globals',
    );
  });
});
