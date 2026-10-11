import type { Event } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultSettings, disconnectedState } from './model';

const transport = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), native: false }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: transport.invoke,
  isTauri: () => transport.native,
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: transport.listen }));

const key = 'knowyourlimits.settings.v1';
let values: Map<string, string>;

beforeEach(() => {
  vi.resetModules();
  transport.native = false;
  transport.invoke.mockReset();
  transport.listen.mockReset();
  values = new Map();
  vi.stubGlobal('localStorage', {
    getItem: (name: string) => values.get(name) ?? null,
    setItem: (name: string, value: string) => {
      values.set(name, value);
    },
  });
});

describe('native bridge', () => {
  it('applies only the requested provider timer through native IPC', async () => {
    transport.native = true;
    transport.listen.mockResolvedValue(() => {});
    const state = disconnectedState(defaultSettings());
    state.settings.preferences.codex.refreshMinutes = 15;
    transport.invoke.mockResolvedValue(state);
    const bridge = await import('./bridge');
    await bridge.setRefreshMinutes('codex', 15);
    expect(transport.invoke).toHaveBeenCalledWith('set_refresh_minutes', {
      provider: 'codex',
      minutes: 15,
    });
    await expect(bridge.setRefreshMinutes('codex', 3)).rejects.toThrow('Select an update interval');
    expect(transport.invoke).toHaveBeenCalledTimes(1);
  });
  it('waits for event registration before initial IPC and registers only once', async () => {
    transport.native = true;
    let register: (() => void) | undefined;
    transport.listen.mockImplementation(
      () =>
        new Promise<() => void>((resolve) => {
          register = () => resolve(() => {});
        }),
    );
    transport.invoke.mockResolvedValue(disconnectedState(defaultSettings()));
    const bridge = await import('./bridge');
    bridge.subscribeState(vi.fn(), vi.fn());
    const initial = bridge.getState();
    expect(transport.invoke).not.toHaveBeenCalled();
    register?.();
    await initial;
    await bridge.getState();
    expect(transport.listen).toHaveBeenCalledTimes(1);
    expect(transport.invoke).toHaveBeenCalledTimes(2);
  });

  it('listens before reading and gates command responses and events on the same revision', async () => {
    transport.native = true;
    const initial = disconnectedState(defaultSettings());
    const newer = {
      ...initial,
      connected: ['codex'],
      revision: 3,
      refreshing: 'codex',
      providers: initial.providers.map((provider) => ({
        ...provider,
        status: 'unavailable',
        windows: [],
        fetchedAt: null,
      })),
    };
    let emit: ((event: Event<unknown>) => void) | undefined;
    transport.listen.mockImplementation(
      async (_name: string, callback: (event: Event<unknown>) => void) => {
        emit = callback;
        return () => {};
      },
    );
    transport.invoke.mockImplementation(async () => {
      expect(emit).toBeDefined();
      emit?.({ event: 'state-changed', id: 1, payload: newer });
      return initial;
    });
    const bridge = await import('./bridge');
    const updates = vi.fn();
    bridge.subscribeState(updates, vi.fn());
    expect((await bridge.getState()).revision).toBe(3);
    expect(updates).toHaveBeenCalledTimes(1);
    const returned = { ...initial, revision: 5 };
    transport.invoke.mockResolvedValue(returned);
    expect((await bridge.readQuota('claude')).revision).toBe(5);
    emit?.({ event: 'state-changed', id: 1, payload: newer });
    expect((await bridge.getState()).revision).toBe(5);
    expect(updates).toHaveBeenCalledTimes(2);
  });

  it('surfaces a static registration notice and sanitizes failed or malformed native commands/events', async () => {
    transport.native = true;
    transport.listen.mockRejectedValue(new Error('/private/auth/token'));
    transport.invoke.mockResolvedValue(disconnectedState(defaultSettings()));
    const bridge = await import('./bridge');
    const notices = vi.fn();
    bridge.subscribeState(vi.fn(), notices);
    await bridge.getState();
    expect(notices).toHaveBeenCalledWith('kyl:errorSubscription');
    transport.invoke.mockRejectedValue(new Error('/private/auth/token'));
    await expect(bridge.readQuota('codex')).rejects.toThrow('Could not complete the action.');
    transport.invoke.mockResolvedValue({ mode: 'live', revision: 9, secret: 'token' });
    await expect(bridge.readQuota('codex')).rejects.toThrow('Could not read usage data.');
  });

  it('rejects malformed events without changing the latest state or leaking the payload', async () => {
    transport.native = true;
    let emit: ((event: Event<unknown>) => void) | undefined;
    transport.listen.mockImplementation(
      async (_name: string, callback: (event: Event<unknown>) => void) => {
        emit = callback;
        return () => {};
      },
    );
    transport.invoke.mockResolvedValue(disconnectedState(defaultSettings()));
    const bridge = await import('./bridge');
    const updates = vi.fn();
    const notices = vi.fn();
    bridge.subscribeState(updates, notices);
    await bridge.getState();
    emit?.({
      event: 'state-changed',
      id: 1,
      payload: { revision: 99, notice: '/private/auth/token' },
    });
    expect(updates).toHaveBeenCalledTimes(1);
    expect(notices).toHaveBeenCalledWith('kyl:invalidState');
    expect((await bridge.getState()).connected).toEqual([]);
  });
  it('opens settings on a tab and follows only known tabs from backend events', async () => {
    transport.native = true;
    let deliver: ((event: Event<unknown>) => void) | undefined;
    transport.listen.mockImplementation(
      async (_name: string, callback: (event: Event<unknown>) => void) => {
        deliver = callback;
        return () => {};
      },
    );
    transport.invoke.mockResolvedValue(undefined);
    vi.stubGlobal('window', { addEventListener: vi.fn() });
    const bridge = await import('./bridge');
    await bridge.openSettings('codex');
    expect(transport.invoke).toHaveBeenCalledWith('open_settings', { tab: 'codex' });
    const tabs = vi.fn();
    bridge.onSettingsTab(tabs);
    expect(transport.listen).toHaveBeenCalledWith('settings-tab', expect.any(Function));
    deliver?.({ event: 'settings-tab', id: 1, payload: 'menubar' });
    deliver?.({ event: 'settings-tab', id: 2, payload: '../private' });
    expect(tabs).toHaveBeenCalledTimes(1);
    expect(tabs).toHaveBeenCalledWith('menubar');
  });
});

describe('browser bridge', () => {
  it('opens settings as a named popup on the requested tab without native IPC', async () => {
    const open = vi.fn();
    vi.stubGlobal('window', { open });
    const bridge = await import('./bridge');
    await bridge.openSettings('claude');
    expect(open).toHaveBeenCalledWith(
      'settings.html#claude',
      'knowyourlimits-settings',
      expect.any(String),
    );
    expect(transport.invoke).not.toHaveBeenCalled();
  });
  it('offers empty state and never invokes native or account requests', async () => {
    const bridge = await import('./bridge');
    await expect(bridge.readQuota('claude')).rejects.toThrow('Available in the desktop app.');
    await expect(bridge.readQuota('codex')).rejects.toThrow('Available in the desktop app.');
    await expect(bridge.signInClaude()).rejects.toThrow('Available in the desktop app.');
    await expect(bridge.cancelRead()).rejects.toThrow('Available in the desktop app.');
    expect(transport.invoke).not.toHaveBeenCalled();
    expect(transport.listen).not.toHaveBeenCalled();
    const state = await bridge.getState();
    expect(state.connected).toEqual([]);
    expect(state.providers.every((p) => p.windows.length === 0)).toBe(true);
  });
  it('persists settings without fabricating snapshots', async () => {
    const bridge = await import('./bridge');
    await bridge.saveSettings({ ...defaultSettings(), barProviders: ['codex'] });
    vi.resetModules();
    const restored = await import('./bridge');
    const state = await restored.getState();
    expect(state.settings.barProviders).toEqual(['codex']);
    expect(state.providers.every((p) => p.windows.length === 0)).toBe(true);
  });
  it('reports corrupted storage and does not overwrite it during initialization', async () => {
    values.set(key, 'broken-json');
    const bridge = await import('./bridge');
    expect((await bridge.getState()).notice).toBe('kyl:defaultSettingsNotice');
    expect(values.get(key)).toBe('broken-json');
  });

  it('reports a write failure without applying settings in memory', async () => {
    const bridge = await import('./bridge');
    await bridge.getState();
    vi.stubGlobal('localStorage', {
      getItem: () => null,
      setItem: () => {
        throw new Error('denied');
      },
    });
    await expect(
      bridge.saveSettings({ ...defaultSettings(), barProviders: ['codex'] }),
    ).rejects.toThrow('Could not save');
    expect((await bridge.getState()).settings.barProviders).toEqual(['claude', 'codex']);
  });

  it('returns isolated copies so renderer changes cannot corrupt backend state', async () => {
    const bridge = await import('./bridge');
    const state = await bridge.getState();
    state.settings.barProviders = ['codex'];
    state.providers.length = 0;
    const after = await bridge.getState();
    expect(after.settings.barProviders).toEqual(['claude', 'codex']);
    expect(after.providers).toHaveLength(2);
  });
});
