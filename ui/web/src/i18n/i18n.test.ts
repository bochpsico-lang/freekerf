import { describe, expect, it } from 'vitest';

import { completeness, LOCALES, localeName, matchLocale, messageIds, setLocale, t } from './index.svelte';

/** Locales FreeKerf translates in full (the others come partly from LaserGRBL). */
const COMPLETE = ['en', 'pt-BR', 'de'];

describe('i18n', () => {
  it('matches locales', () => {
    expect(matchLocale('pt-BR')).toBe('pt-BR');
    expect(matchLocale('pt-PT')).toBe('pt-BR');
    expect(matchLocale('de-AT')).toBe('de');
    expect(matchLocale('ja')).toBe('ja-JP');
    expect(matchLocale('ko')).toBe('en');
    expect(matchLocale(undefined)).toBe('en');
  });

  it('formats with arguments and falls back to English, then to the id', () => {
    setLocale('pt-BR');
    expect(t('machine-start')).toBe('Iniciar');
    expect(t('machine-line', { a: 3, b: 10 })).toBe('Linha 3 de 10');
    setLocale('en');
    expect(t('machine-line', { a: 3, b: 10 })).toBe('Line 3 of 10');
    expect(t('no-such-message')).toBe('no-such-message');
  });

  it('lists the locales of ui/i18n, English first, with LaserGRBL ones', () => {
    expect(LOCALES[0]).toBe('en');
    expect(LOCALES).toEqual(expect.arrayContaining(['pt-BR', 'de', 'fr', 'ja-JP']));
    expect(localeName('de')).toBe('Deutsch');
    expect(completeness('pt-BR')).toBe(1);
    expect(completeness('fr')).toBeGreaterThan(0);
    expect(completeness('fr')).toBeLessThan(0.5);
    setLocale('fr');
    expect(t('machine-connect')).toBe('Connecter');
    expect(t('mode-produce')).toBe('Produce');
    setLocale('auto');
    setLocale('en');
  });

  it('en, pt-BR and de are complete', () => {
    const en = new Set(messageIds('en'));
    expect(en.size).toBeGreaterThan(200);
    for (const l of COMPLETE) {
      const ids = new Set(messageIds(l));
      const missing = [...en].filter((id) => !ids.has(id));
      expect(missing, l).toEqual([]);
    }
  });
});

describe('message ids used in the code', () => {
  it('exist in English', async () => {
    const { readdirSync, readFileSync, statSync } = await import('node:fs');
    const { join } = await import('node:path');
    const walk = (d: string): string[] =>
      readdirSync(d).flatMap((f) => {
        const p = join(d, f);
        if (statSync(p).isDirectory()) return f === 'gen' ? [] : walk(p);
        return /\.(ts|svelte)$/.test(f) && !f.endsWith('.test.ts') ? [p] : [];
      });
    const en = new Set(messageIds('en'));
    const missing: string[] = [];
    for (const file of walk(join(process.cwd(), 'src'))) {
      const text = readFileSync(file, 'utf8');
      for (const m of text.matchAll(/\bt\(\s*'([a-zA-Z][\w-]*)'/g)) {
        if (!en.has(m[1]!)) missing.push(`${file.split('/src/')[1]}: ${m[1]}`);
      }
    }
    expect(missing).toEqual([]);
  });
});
