import {
  type AppState,
  defaultSettings,
  disconnectedState,
  type ProviderStatus,
  type Settings,
} from '../src/model';
export function sampleState(
  status: ProviderStatus = 'current',
  settings: Settings = defaultSettings(),
  now = Date.now(),
): AppState {
  const state = disconnectedState(settings);
  state.systemLocale = 'en';
  state.connected = ['claude', 'codex'];
  state.providers = state.providers.map((p, i) => ({
    ...p,
    status,
    fetchedAt:
      status === 'unavailable'
        ? null
        : now - (status === 'stale' ? 600_000 : status === 'error' ? 120_000 : 0),
    windows:
      status === 'unavailable'
        ? []
        : [
            {
              id: 'session',
              label: '5 часов',
              usedPercent: i === 0 ? 74 : 62,
              resetsAt: now + 80 * 60_000,
              durationMins: 300,
              primary: true,
            },
            {
              id: 'week',
              label: 'Неделя',
              usedPercent: 18,
              resetsAt: now + 604_800_000,
              durationMins: 10080,
              primary: true,
            },
          ],
  }));
  return state;
}
