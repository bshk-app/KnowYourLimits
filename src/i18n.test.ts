import { afterEach, describe, expect, it } from 'vitest';
import {
  getLocale,
  isLanguage,
  isNoticeCode,
  LANGUAGES,
  localizeNotice,
  percent,
  resolveLocale,
  setLanguage,
  t,
  unit,
  windowLabel,
} from './i18n';
import { catalogs } from './locales';
import { defaultSettings, formatAge, formatReset, validateSettings } from './model';

afterEach(() => setLanguage('en'));

describe('localization', () => {
  it('includes all 24 EU official languages and Russian, with matching placeholders', () => {
    expect((Object.keys(LANGUAGES) as (keyof typeof LANGUAGES)[]).sort()).toEqual(
      'bg cs da de el en es et fi fr ga hr hu it lt lv mt nl pl pt ro ru sk sl sv'.split(' '),
    );
    const english = catalogs.en;
    for (const [locale, catalog] of Object.entries(catalogs)) {
      expect(Object.keys(catalog).sort(), locale).toEqual(Object.keys(english).sort());
      for (const key of Object.keys(english) as (keyof typeof english)[]) {
        expect(catalog[key].trim().length, `${locale}.${key}`).toBeGreaterThan(0);
        expect(catalog[key].match(/\{\w+\}/g)?.sort() ?? [], `${locale}.${key}`).toEqual(
          english[key].match(/\{\w+\}/g)?.sort() ?? [],
        );
      }
    }
  });
  it('matches regional system languages in preference order and falls back to English', () => {
    expect(resolveLocale('system', ['ja-JP', 'fi-FI', 'ru'])).toBe('fi');
    expect(resolveLocale('system', ['PT_pt'])).toBe('pt');
    expect(resolveLocale('system', ['zh-Hant'])).toBe('en');
    expect(resolveLocale('ru', ['fi-FI'])).toBe('ru');
    expect(isLanguage('__proto__')).toBe(false);
    expect(isLanguage('unknown')).toBe(false);
  });
  it('preserves old settings and validates the new language preference', () => {
    const { language: _language, ...legacy } = defaultSettings();
    expect(validateSettings(legacy).language).toBe('system');
    expect(validateSettings({ ...legacy, language: 'mt' }).language).toBe('mt');
    expect(() => validateSettings({ ...legacy, language: 'xx' })).toThrow();
    expect(() => validateSettings({ ...legacy, language: null })).toThrow();
  });
  it('localizes interpolation, native notices and quota labels without cached Russian labels', () => {
    setLanguage('de');
    expect(getLocale()).toBe('de');
    expect(t('connect', { provider: 'Claude' })).toContain('Claude');
    expect(windowLabel({ durationMins: 10080, label: 'Неделя' })).toBe('Woche');
    expect(windowLabel({ durationMins: 300, label: '5 часов' })).toBe('5 Stunden');
    expect(windowLabel({ durationMins: 0, label: 'Неизвестная длительность' })).toBe('Zeitraum');
    expect(localizeNotice('kyl:cancelled kyl:errorCacheSave')).toBe(
      `${t('cancelled')} ${t('errorCacheSave')}`,
    );
    expect(isNoticeCode('kyl:cancelled')).toBe(true);
    expect(isNoticeCode('kyl:cancelled private-data')).toBe(false);
    expect(isNoticeCode('private-data')).toBe(false);
    expect(percent(34)).toMatch(/34\s%/);
  });
  it('uses language-aware plural forms and never invents a reset', () => {
    setLanguage('ru');
    expect(unit(1, 'hour', 'long')).toBe('1 час');
    expect(unit(2, 'hour', 'long')).toBe('2 часа');
    expect(unit(5, 'hour', 'long')).toBe('5 часов');
    expect(formatAge(0, 21 * 60_000)).toBe('21 минуту назад');
    expect(formatReset(null, 0)).toBe(t('resetUnknown'));
    expect(formatReset(0, 0)).toBe(t('resetPending'));
    expect(formatReset(60_000, 0)).toContain(unit(1, 'minute'));
  });
});
