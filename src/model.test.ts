import { describe, expect, it } from 'vitest';
import { sampleState } from '../tests/fixtures';
import {
  AUTO_REFRESH_MINUTES,
  defaultSettings,
  formatReset,
  isProviderFresh,
  parseThresholds,
  refreshLabel,
  remainingPercent,
  trayTitle,
  validateAppState,
  validateSettings,
} from './model';

const NOW = 1_800_000_000_000;

describe('native state validation', () => {
  it('rebuilds allowlisted state and nested fields without retaining transport objects', () => {
    const source = sampleState('current', defaultSettings(), NOW);
    const input = {
      ...source,
      revision: 7,
      secret: 'discard',
      settings: { ...source.settings, secret: 'discard' },
      providers: source.providers.map((provider) => ({
        ...provider,
        secret: 'discard',
        windows: provider.windows.map((window) => ({ ...window, secret: 'discard' })),
      })),
    };
    const result = validateAppState(input);
    expect(result.connected).toEqual(['claude', 'codex']);
    expect(result.revision).toBe(7);
    expect(result).not.toHaveProperty('secret');
    expect(result.settings).not.toHaveProperty('secret');
    expect(result.providers[0]).not.toHaveProperty('secret');
    expect(result.providers[0]?.windows[0]).not.toHaveProperty('secret');
    expect(result.providers).not.toBe(input.providers);
    expect(result.providers[0]?.windows).not.toBe(input.providers[0]?.windows);
    expect(result.settings).not.toBe(input.settings);
  });

  it('rejects the entire payload on malformed enums, numbers, windows or settings', () => {
    const state = {
      ...sampleState('current', defaultSettings(), NOW),
      refreshing: null,
      revision: 7,
    };
    for (const patch of [
      { connected: undefined },
      { connected: ['other'] },
      { connected: ['claude', 'claude'] },
      { signingIn: undefined },
      { refreshing: 1 },
      { revision: -1 },
      { revision: 1.5 },
      { revision: Number.NaN },
      { notice: {} },
      { settings: {} },
      { providers: [] },
      { providers: [state.providers[0], state.providers[0]] },
      { providers: [{ ...state.providers[0], status: 'unknown' }, state.providers[1]] },
      { providers: [{ ...state.providers[0], fetchedAt: Infinity }, state.providers[1]] },
      {
        providers: [
          {
            ...state.providers[0],
            windows: [
              {
                id: 'week',
                label: 'Weekly',
                usedPercent: 101,
                resetsAt: null,
                durationMins: 10080,
                primary: true,
              },
            ],
          },
          state.providers[1],
        ],
      },
      {
        providers: [
          {
            ...state.providers[0],
            windows: [
              {
                id: 'week',
                label: 'Weekly',
                usedPercent: 23,
                resetsAt: null,
                durationMins: -1,
                primary: 'yes',
              },
            ],
          },
          state.providers[1],
        ],
      },
    ])
      expect(() => validateAppState({ ...state, ...patch })).toThrow('Could not read usage data.');
  });
});

describe('remaining quota', () => {
  it('keeps the last percentage visible when a manual snapshot ages', () => {
    const state = sampleState('current', defaultSettings(), NOW);
    expect(trayTitle(state, NOW + 900_000)).toBe('C ~26% · X ~38%');
    expect(state.providers[1]?.fetchedAt).toBe(NOW);
  });
  it('takes the lowest remaining window, not an average or the short window', () => {
    const state = sampleState('current', defaultSettings(), NOW);
    expect(trayTitle(state, NOW)).toBe('C 26% · X 38%');
  });

  it('does not present an old live snapshot as current while refreshing', () => {
    const state = sampleState('current', defaultSettings(), NOW);
    state.refreshing = 'codex';
    expect(trayTitle(state, NOW)).toBe('C 26% · X ~38%');
  });

  it('preserves selection and order', () => {
    const settings = { ...defaultSettings(), barProviders: ['codex', 'claude'] as const };
    const state = sampleState(
      'current',
      { ...settings, barProviders: [...settings.barProviders] },
      NOW,
    );
    expect(trayTitle(state, NOW)).toBe('X 38% · C 26%');
    state.settings.barProviders = ['codex'];
    expect(trayTitle(state, NOW)).toBe('X 38%');
  });

  it.each(['stale', 'error', 'unavailable'] as const)('does not imply zero for %s', (scenario) => {
    expect(trayTitle(sampleState(scenario, defaultSettings(), NOW), NOW)).toBe(
      scenario === 'unavailable' ? 'C — · X —' : 'C ~26% · X ~38%',
    );
  });

  it('keeps historical windows on error and stale states', () => {
    for (const scenario of ['error', 'stale'] as const) {
      const state = sampleState(scenario, defaultSettings(), NOW);
      expect(state.providers[0]?.windows[0]?.usedPercent).toBe(74);
    }
  });

  it('expires independently by timestamp', () => {
    const state = sampleState('current', defaultSettings(), NOW);
    expect(trayTitle(state, NOW + 300_001)).toBe('C ~26% · X ~38%');
    const claude = state.providers[0];
    if (!claude) throw new Error('missing fixture');
    claude.fetchedAt = NOW - 300_001;
    expect(trayTitle(state, NOW)).toBe('C ~26% · X 38%');
  });

  it('rejects future samples', () => {
    const provider = sampleState('current', defaultSettings(), NOW).providers[0];
    if (!provider) throw new Error('missing fixture');
    provider.fetchedAt = NOW + 1;
    expect(isProviderFresh(provider, NOW)).toBe(false);
  });

  it('shows missing windows as unavailable, not unused', () => {
    const state = sampleState('current', defaultSettings(), NOW);
    const provider = state.providers[0];
    if (!provider) throw new Error('missing fixture');
    provider.windows = [];
    expect(remainingPercent(provider, NOW)).toBeNull();
  });

  it('excludes elapsed resets, invalid percentages and non-primary buckets', () => {
    const provider = sampleState('current', defaultSettings(), NOW).providers[0];
    if (!provider) throw new Error('missing fixture');
    const first = provider.windows[0];
    const second = provider.windows[1];
    if (!first || !second) throw new Error('missing windows');
    first.resetsAt = NOW;
    expect(remainingPercent(provider, NOW)).toBe(82);
    second.usedPercent = Number.NaN;
    expect(remainingPercent(provider, NOW)).toBeNull();
    second.usedPercent = 101;
    expect(remainingPercent(provider, NOW)).toBeNull();
    second.usedPercent = -1;
    expect(remainingPercent(provider, NOW)).toBeNull();
    second.usedPercent = 90;
    second.primary = false;
    expect(remainingPercent(provider, NOW)).toBeNull();
  });

  it('allows an unknown reset time, but never invents one', () => {
    const provider = sampleState('current', defaultSettings(), NOW).providers[0];
    if (!provider) throw new Error('missing fixture');
    for (const window of provider.windows) window.resetsAt = null;
    expect(remainingPercent(provider, NOW)).toBe(26);
    expect(formatReset(null, NOW)).toBe('Reset time unavailable');
  });

  it('retains fractional accuracy until display formatting', () => {
    const provider = sampleState('current', defaultSettings(), NOW).providers[0];
    if (!provider?.windows[0]) throw new Error('missing fixture');
    provider.windows[0].usedPercent = 95.4;
    expect(remainingPercent(provider, NOW)).toBeCloseTo(4.6);
  });
});

describe('settings', () => {
  it('validates and sorts thresholds without mutating the caller', () => {
    const settings = defaultSettings();
    settings.thresholds.claude = [100, 80, 95];
    expect(validateSettings(settings).thresholds.claude).toEqual([80, 95, 100]);
    expect(settings.thresholds.claude).toEqual([100, 80, 95]);
  });

  it.each(
    [[], ['claude', 'claude'], ['unknown'], ['claude', 'codex', 'claude']].map((barProviders) => ({
      barProviders,
    })),
  )('rejects invalid provider selections $barProviders', ({ barProviders }) =>
    expect(() => validateSettings({ ...defaultSettings(), barProviders })).toThrow(),
  );

  it.each(
    [[0], [101], [80.5], [80, 80], [], [1, 2, 3, 4, 5, 6], ['80'], [null]].map((claude) => ({
      claude,
    })),
  )('rejects invalid thresholds $claude', ({ claude }) =>
    expect(() =>
      validateSettings({ ...defaultSettings(), thresholds: { claude, codex: [80] } }),
    ).toThrow(),
  );

  it('rejects malformed persisted settings', () => {
    for (const data of [null, 'oops', {}, { barProviders: ['claude'] }]) {
      expect(() => validateSettings(data)).toThrow();
    }
  });

  it('parses editable thresholds strictly', () => {
    expect(parseThresholds('100, 80, 95')).toEqual([80, 95, 100]);
    for (const input of ['80abc', '80,', ',80', '80,,95', '80 95', '0', '80,80', '']) {
      expect(() => parseThresholds(input)).toThrow();
    }
  });
});

describe('reset countdown', () => {
  it('does not invent a recovered limit after the reset', () => {
    expect(formatReset(NOW, NOW)).toBe('Awaiting reset');
    expect(formatReset(NOW - 1, NOW)).toBe('Awaiting reset');
  });

  it('formats minutes, hours and days', () => {
    expect(formatReset(NOW + 42 * 60_000, NOW)).toBe('Resets in 42 min');
    expect(formatReset(NOW + 80 * 60_000, NOW)).toBe('Resets in 1 hr 20 min');
    expect(formatReset(NOW + 4560 * 60_000, NOW)).toBe('Resets in 3 days 4 hr');
  });
});

describe('provider preferences', () => {
  it('migrates legacy settings and retains independent thresholds', () => {
    const result = validateSettings({
      barProviders: ['codex'],
      thresholds: { claude: [70, 90], codex: [75, 95] },
    });
    expect(result.barProviders).toEqual(['codex']);
    expect(result.thresholds.claude).toEqual([70, 90]);
    expect(result.layout).toBe('stacked');
    expect(result.preferences.claude.refreshMinutes).toBe(0);
    expect(result.preferences.codex.notifications.enabled).toBe(false);
  });
  it('rebuilds independent presentation timer and notification preferences', () => {
    const settings = defaultSettings();
    settings.layout = 'rings';
    settings.preferences.claude.color = '#AA6633';
    settings.preferences.claude.size = 'large';
    settings.preferences.claude.icon = 'dot';
    settings.preferences.claude.refreshMinutes = 2;
    settings.preferences.claude.notifications = { enabled: true, sound: true, recovery: true };
    settings.preferences.codex.refreshMinutes = 15;
    const result = validateSettings(settings);
    expect(result.preferences.claude.color).toBe('#aa6633');
    expect(result.preferences.codex.color).toBe('system');
    expect(result.preferences.codex.notifications.enabled).toBe(false);
    expect(result.preferences.codex.refreshMinutes).toBe(15);
    expect(result.preferences.claude).not.toBe(settings.preferences.claude);
  });
  it('accepts the adaptive interval and labels every timer option', () => {
    const settings = defaultSettings();
    settings.preferences.claude.refreshMinutes = AUTO_REFRESH_MINUTES;
    expect(validateSettings(settings).preferences.claude.refreshMinutes).toBe(AUTO_REFRESH_MINUTES);
    expect([0, AUTO_REFRESH_MINUTES, 1, 15].map(refreshLabel)).toEqual([
      'Off',
      'Auto: 5–15 min',
      'Every 1 min',
      'Every 15 min',
    ]);
  });
  it('rejects malformed new fields instead of losing the whole saved configuration', () => {
    for (const patch of [
      { color: 'red' },
      { color: '#123' },
      { color: '#aabbff<script>' },
      { icon: 'other' },
      { size: 'huge' },
      { refreshMinutes: 3 },
      { refreshMinutes: 1.5 },
      { refreshMinutes: AUTO_REFRESH_MINUTES - 1 },
      { notifications: { enabled: 'yes', sound: false, recovery: false } },
    ]) {
      const settings = defaultSettings();
      expect(() =>
        validateSettings({
          ...settings,
          preferences: {
            ...settings.preferences,
            claude: { ...settings.preferences.claude, ...patch },
          },
        }),
      ).toThrow();
    }
    expect(() => validateSettings({ ...defaultSettings(), layout: 'unknown' })).toThrow();
  });
});

describe('quota cooldown', () => {
  it('validates retry deadlines and supplies an empty map for legacy payloads', () => {
    const state = sampleState();
    state.retryAt.claude = NOW + 300000;
    expect(validateAppState(state).retryAt.claude).toBe(NOW + 300000);
    const old = { ...state, retryAt: undefined };
    expect(validateAppState(old).retryAt).toEqual({ claude: null, codex: null });
    for (const invalid of [
      { claude: -1, codex: null },
      { claude: 'soon', codex: null },
      { claude: Infinity, codex: null },
      { claude: NOW + 0.5, codex: null },
    ])
      expect(() => validateAppState({ ...state, retryAt: invalid })).toThrow(
        'Could not read usage data.',
      );
  });
});
