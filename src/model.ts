import {
  duration,
  getLocale,
  isLanguage,
  type Language,
  type Locale,
  resolveLocale,
  t,
  unit,
} from './i18n';
export const PROVIDER_IDS = ['claude', 'codex'] as const;
export type ProviderId = (typeof PROVIDER_IDS)[number];
export type ProviderStatus =
  | 'current'
  | 'blocked'
  | 'stale'
  | 'error'
  | 'unavailable'
  | 'signedout';
export const STALE_AFTER_MS = 5 * 60_000;

export const PROVIDER_LABELS: Record<ProviderId, string> = { claude: 'Claude', codex: 'Codex' };
export const PROVIDER_SHORT: Record<ProviderId, string> = { claude: 'C', codex: 'X' };
export const LAYOUTS = ['inline', 'stacked', 'rings'] as const;
export type TrayLayout = (typeof LAYOUTS)[number];
export const ICONS = ['symbol', 'letter', 'dot', 'none'] as const;
export type IconStyle = (typeof ICONS)[number];
export const SIZES = ['small', 'medium', 'large'] as const;
export type IndicatorSize = (typeof SIZES)[number];
/** Adaptive timer, same value as the native setting: 5 min after recent panel use, else 15. */
export const AUTO_REFRESH_MINUTES = 65535;
export const REFRESH_MINUTES = [0, AUTO_REFRESH_MINUTES, 1, 2, 5, 10, 15, 30, 60] as const;
export function refreshLabel(minutes: number): string {
  if (minutes === 0) return t('off');
  if (minutes === AUTO_REFRESH_MINUTES) return t('adaptive');
  return t('every', { time: unit(minutes, 'minute') });
}
export interface ProviderPreferences {
  icon: IconStyle;
  color: string;
  size: IndicatorSize;
  refreshMinutes: number;
  notifications: { enabled: boolean; sound: boolean; recovery: boolean };
}

export interface Settings {
  language: Language;
  barProviders: ProviderId[];
  thresholds: Record<ProviderId, number[]>;
  layout: TrayLayout;
  preferences: Record<ProviderId, ProviderPreferences>;
}

export interface QuotaWindow {
  id: string;
  label: string;
  usedPercent: number;
  resetsAt: number | null;
  durationMins: number;
  primary: boolean;
}

export interface ProviderSnapshot {
  id: ProviderId;
  label: string;
  status: ProviderStatus;
  fetchedAt: number | null;
  windows: QuotaWindow[];
}

export interface AppState {
  systemLocale: Locale;
  connected: ProviderId[];
  refreshing: ProviderId | null;
  signingIn: boolean;
  revision: number;
  settings: Settings;
  providers: ProviderSnapshot[];
  notice: string | null;
  retryAt: Record<ProviderId, number | null>;
}

export function defaultSettings(): Settings {
  const preferences = (): ProviderPreferences => ({
    icon: 'symbol',
    color: 'system',
    size: 'medium',
    refreshMinutes: 0,
    notifications: { enabled: false, sound: false, recovery: false },
  });
  return {
    language: 'system',
    barProviders: ['claude', 'codex'],
    thresholds: { claude: [80, 95, 100], codex: [80, 95, 100] },
    layout: 'stacked',
    preferences: { claude: preferences(), codex: preferences() },
  };
}

export function isProviderId(value: unknown): value is ProviderId {
  return value === 'claude' || value === 'codex';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function validateThresholds(value: unknown): number[] {
  if (
    !Array.isArray(value) ||
    value.length < 1 ||
    value.length > 5 ||
    !value.every(
      (entry: unknown) =>
        typeof entry === 'number' && Number.isInteger(entry) && entry >= 1 && entry <= 100,
    ) ||
    new Set(value).size !== value.length
  ) {
    throw new Error(t('invalidThresholds'));
  }
  return [...value].sort((a, b) => a - b);
}

function validatePreferences(value: unknown): ProviderPreferences {
  if (
    !isRecord(value) ||
    !ICONS.some((icon) => icon === value.icon) ||
    !SIZES.some((size) => size === value.size) ||
    typeof value.color !== 'string' ||
    (value.color !== 'system' && !/^#[0-9a-f]{6}$/i.test(value.color)) ||
    !REFRESH_MINUTES.some((minutes) => minutes === value.refreshMinutes) ||
    !isRecord(value.notifications) ||
    !['enabled', 'sound', 'recovery'].every(
      (key) =>
        typeof value.notifications === 'object' &&
        value.notifications !== null &&
        typeof (value.notifications as Record<string, unknown>)[key] === 'boolean',
    )
  ) {
    throw new Error(t('invalidPreferences'));
  }
  return {
    icon: value.icon as IconStyle,
    color: value.color.toLowerCase(),
    size: value.size as IndicatorSize,
    refreshMinutes: value.refreshMinutes as number,
    notifications: {
      enabled: value.notifications.enabled as boolean,
      sound: value.notifications.sound as boolean,
      recovery: value.notifications.recovery as boolean,
    },
  };
}

export function validateSettings(value: unknown): Settings {
  if (!isRecord(value) || !isRecord(value.thresholds)) throw new Error(t('errorSettings'));
  const language = value.language === undefined ? 'system' : value.language;
  if (!isLanguage(language)) throw new Error(t('invalidLanguage'));
  const providers: unknown = value.barProviders;
  if (
    !Array.isArray(providers) ||
    providers.length < 1 ||
    providers.length > 2 ||
    !providers.every(isProviderId) ||
    new Set(providers).size !== providers.length
  ) {
    throw new Error(t('invalidProviders'));
  }
  const defaults = defaultSettings();
  const layout = value.layout === undefined ? defaults.layout : value.layout;
  if (
    !LAYOUTS.some((candidate) => candidate === layout) ||
    (value.preferences !== undefined && !isRecord(value.preferences))
  )
    throw new Error(t('invalidIndicator'));
  const preferences =
    value.preferences === undefined
      ? defaults.preferences
      : {
          claude: validatePreferences((value.preferences as Record<string, unknown>).claude),
          codex: validatePreferences((value.preferences as Record<string, unknown>).codex),
        };
  return {
    language,
    barProviders: [...providers],
    thresholds: {
      claude: validateThresholds(value.thresholds.claude),
      codex: validateThresholds(value.thresholds.codex),
    },
    layout: layout as TrayLayout,
    preferences,
  };
}

const invalidState = () => t('invalidState');

function stateNumber(value: unknown, max = Number.MAX_SAFE_INTEGER): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0 || value > max)
    throw new Error(invalidState());
  return value;
}

function stateInteger(value: unknown): number {
  const number = stateNumber(value);
  if (!Number.isSafeInteger(number)) throw new Error(invalidState());
  return number;
}

function stateText(value: unknown): string {
  if (typeof value !== 'string' || !value.trim() || value.length > 500)
    throw new Error(invalidState());
  return value;
}

function stateTimestamp(value: unknown): number | null {
  return value === null ? null : stateInteger(value);
}

function validateWindow(value: unknown): QuotaWindow {
  if (!isRecord(value) || typeof value.primary !== 'boolean') throw new Error(invalidState());
  return {
    id: stateText(value.id),
    label: stateText(value.label),
    usedPercent: stateNumber(value.usedPercent, 100),
    resetsAt: stateTimestamp(value.resetsAt),
    durationMins: stateInteger(value.durationMins),
    primary: value.primary,
  };
}

function validateProvider(value: unknown): ProviderSnapshot {
  if (
    !isRecord(value) ||
    !isProviderId(value.id) ||
    !Array.isArray(value.windows) ||
    (value.status !== 'current' &&
      value.status !== 'blocked' &&
      value.status !== 'stale' &&
      value.status !== 'error' &&
      value.status !== 'unavailable' &&
      value.status !== 'signedout')
  )
    throw new Error(invalidState());
  const windows = value.windows.map(validateWindow);
  if (new Set(windows.map((window) => window.id)).size !== windows.length)
    throw new Error(invalidState());
  return {
    id: value.id,
    label: stateText(value.label),
    status: value.status,
    fetchedAt: stateTimestamp(value.fetchedAt),
    windows,
  };
}

// Rebuild, never cast an IPC object: no extra transport fields cross this boundary.
export function validateAppState(value: unknown): AppState {
  try {
    if (
      !isRecord(value) ||
      !Array.isArray(value.connected) ||
      !value.connected.every(isProviderId) ||
      new Set(value.connected).size !== value.connected.length ||
      (value.refreshing !== null && !isProviderId(value.refreshing)) ||
      typeof value.signingIn !== 'boolean' ||
      !Array.isArray(value.providers)
    )
      throw new Error(invalidState());
    const providers = value.providers.map(validateProvider);
    if (providers.length !== 2 || new Set(providers.map((provider) => provider.id)).size !== 2)
      throw new Error(invalidState());
    return {
      systemLocale:
        isLanguage(value.systemLocale) && value.systemLocale !== 'system'
          ? value.systemLocale
          : resolveLocale('system'),
      connected: [...value.connected],
      signingIn: value.signingIn,
      refreshing: value.refreshing,
      revision: stateInteger(value.revision),
      settings: validateSettings(value.settings),
      providers,
      notice: value.notice === null ? null : stateText(value.notice),
      retryAt:
        value.retryAt === undefined
          ? { claude: null, codex: null }
          : validateRetryAt(value.retryAt),
    };
  } catch {
    throw new Error(invalidState());
  }
}

function validateRetryAt(value: unknown): Record<ProviderId, number | null> {
  if (!isRecord(value)) throw new Error(invalidState());
  return { claude: stateTimestamp(value.claude), codex: stateTimestamp(value.codex) };
}
export function isProviderPaused(state: AppState, id: ProviderId, now = Date.now()): boolean {
  const until = state.retryAt[id];
  return until !== null && until > now;
}
export function formatRetry(until: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.ceil((until - now) / 1000));
  return t('retryIn', {
    time: seconds >= 60 ? unit(Math.ceil(seconds / 60), 'minute') : unit(seconds, 'second'),
  });
}

export function parseThresholds(input: string): number[] {
  const parts = input.split(',').map((part) => part.trim());
  if (!parts.every((part) => /^\d{1,3}$/.test(part))) {
    throw new Error(t('thresholdExample'));
  }
  return validateThresholds(parts.map(Number));
}

export function disconnectedState(settings = defaultSettings()): AppState {
  return {
    systemLocale: resolveLocale('system'),
    connected: [],
    refreshing: null,
    signingIn: false,
    revision: 0,
    settings: validateSettings(settings),
    notice: null,
    retryAt: { claude: null, codex: null },
    providers: PROVIDER_IDS.map((id) => ({
      id,
      label: PROVIDER_LABELS[id],
      status: 'unavailable',
      fetchedAt: null,
      windows: [],
    })),
  };
}

export function isProviderFresh(provider: ProviderSnapshot, now = Date.now()): boolean {
  return (
    (provider.status === 'current' || provider.status === 'blocked') &&
    provider.fetchedAt !== null &&
    Number.isFinite(provider.fetchedAt) &&
    provider.fetchedAt <= now &&
    now - provider.fetchedAt <= STALE_AFTER_MS
  );
}

export function remainingPercent(provider: ProviderSnapshot, now = Date.now()): number | null {
  if (!isProviderFresh(provider, now)) return null;
  const available = provider.windows.filter(
    (window) =>
      window.primary &&
      Number.isFinite(window.usedPercent) &&
      window.usedPercent >= 0 &&
      window.usedPercent <= 100 &&
      (window.resetsAt === null || (Number.isFinite(window.resetsAt) && window.resetsAt > now)),
  );
  return available.length ? Math.min(...available.map((window) => 100 - window.usedPercent)) : null;
}

export function trayTitle(state: AppState, now = Date.now()): string {
  return state.settings.barProviders
    .map((id) => {
      const provider = state.providers.find((entry) => entry.id === id);
      const remaining = provider ? displayRemainingPercent(provider, now) : null;
      const historical =
        state.refreshing === id || !provider || remainingPercent(provider, now) === null;
      return `${PROVIDER_SHORT[id]} ${remaining === null ? '—' : `${historical ? '~' : ''}${Math.round(remaining)}%`}`;
    })
    .join(' · ');
}

export function displayRemainingPercent(
  provider: ProviderSnapshot,
  now = Date.now(),
): number | null {
  const current = remainingPercent(provider, now);
  if (current !== null) return current;
  if (provider.status === 'signedout' || provider.fetchedAt === null || provider.fetchedAt > now)
    return null;
  const windows = provider.windows.filter(
    (window) =>
      window.primary &&
      Number.isFinite(window.usedPercent) &&
      window.usedPercent >= 0 &&
      window.usedPercent <= 100,
  );
  return windows.length ? Math.min(...windows.map((window) => 100 - window.usedPercent)) : null;
}

export function quotaTone(
  used: number,
  thresholds: number[],
): 'normal' | 'warning' | 'critical' | 'exhausted' {
  if (used >= 100) return 'exhausted';
  const first = thresholds[0] ?? 80;
  const second = thresholds[1];
  if (second !== undefined && used >= second) return 'critical';
  return used >= first ? 'warning' : 'normal';
}

export function formatReset(resetsAt: number | null, now = Date.now()): string {
  if (resetsAt === null || !Number.isFinite(resetsAt)) return t('resetUnknown');
  if (resetsAt <= now) return t('resetPending');
  return t('resetIn', { time: duration(Math.ceil((resetsAt - now) / 60_000)) });
}
export function formatAge(fetchedAt: number | null, now = Date.now()): string {
  if (fetchedAt === null) return t('noData');
  const minutes = Math.max(0, Math.floor((now - fetchedAt) / 60_000));
  if (minutes === 0) return t('justNow');
  const formatter = new Intl.RelativeTimeFormat(getLocale(), { numeric: 'always' });
  if (minutes >= 1440) return formatter.format(-Math.floor(minutes / 1440), 'day');
  if (minutes >= 60) return formatter.format(-Math.floor(minutes / 60), 'hour');
  return formatter.format(-minutes, 'minute');
}
