import { describe, expect, it } from 'vitest';

import { LOCALES, matchLocale, messageIds, setLocale, t } from './index.svelte';

describe('i18n', () => {
  it('matches locales', () => {
    expect(matchLocale('pt-BR')).toBe('pt-BR');
    expect(matchLocale('pt-PT')).toBe('pt-BR');
    expect(matchLocale('de-AT')).toBe('de');
    expect(matchLocale('ja')).toBe('en');
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

  it('every English message exists in the other locales', () => {
    const en = new Set(messageIds('en'));
    expect(en.size).toBeGreaterThan(200);
    for (const l of LOCALES) {
      const ids = new Set(messageIds(l));
      const missing = [...en].filter((id) => !ids.has(id));
      expect(missing, l).toEqual([]);
    }
  });
});
