import type { Event } from '@tauri-apps/api/event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { UpdateState } from './update-bridge';

const transport = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock('./bridge', () => ({ isNative: true }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: transport.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: transport.listen }));
const state = (revision: number, status: UpdateState['status']): UpdateState => ({
  revision,
  status,
  supported: true,
  installation: 'managed',
  version: status === 'available' ? '0.2.8' : null,
  downloaded: 0,
  total: null,
  notice: null,
});
beforeEach(() => {
  vi.resetModules();
  transport.invoke.mockReset();
  transport.listen.mockReset();
  transport.listen.mockResolvedValue(() => {});
});
afterEach(() => {
  vi.useRealTimers();
});
describe('app updater bridge', () => {
  it('keeps newer events when a command response arrives late', async () => {
    let emit: ((e: Event<unknown>) => void) | undefined;
    transport.listen.mockImplementation(async (_: string, callback: typeof emit) => {
      emit = callback;
      return () => {};
    });
    transport.invoke.mockImplementation(async () => {
      emit?.({ event: 'updates-changed', id: 1, payload: state(2, 'available') });
      return state(1, 'checking');
    });
    const bridge = await import('./update-bridge');
    const changes = vi.fn();
    const unsubscribe = bridge.subscribeUpdates(changes);
    expect((await bridge.checkForUpdates()).status).toBe('available');
    expect(changes).toHaveBeenCalledTimes(1);
    unsubscribe();
  });
  it('retries a failed listener and polls an in-flight command instead of leaving controls checking forever', async () => {
    vi.useFakeTimers();
    transport.listen.mockRejectedValueOnce(new Error('/private/token')).mockResolvedValue(() => {});
    transport.invoke
      .mockResolvedValueOnce(state(1, 'checking'))
      .mockResolvedValue(state(2, 'available'));
    const bridge = await import('./update-bridge');
    const changes = vi.fn();
    const unsubscribe = bridge.subscribeUpdates(changes);
    await bridge.checkForUpdates();
    await vi.advanceTimersByTimeAsync(1000);
    expect(transport.listen).toHaveBeenCalledTimes(2);
    expect(transport.invoke).toHaveBeenCalledWith('get_update_state');
    expect(changes).toHaveBeenLastCalledWith(state(2, 'available'));
    unsubscribe();
  });
  it('bounds failed polling and reports only a static recoverable notice', async () => {
    vi.useFakeTimers();
    transport.listen.mockRejectedValue(new Error('/private/auth'));
    transport.invoke
      .mockResolvedValueOnce(state(1, 'downloading'))
      .mockRejectedValue(new Error('/private/auth'));
    const bridge = await import('./update-bridge');
    const notice = vi.fn();
    const unsubscribe = bridge.subscribeUpdates(vi.fn(), notice);
    await bridge.downloadUpdate();
    await vi.advanceTimersByTimeAsync(3000);
    expect(notice).toHaveBeenCalledWith(bridge.updateText.notices.requestFailed);
    expect(notice.mock.calls.flat().join(' ')).not.toContain('/private');
    const calls = transport.invoke.mock.calls.length;
    await vi.advanceTimersByTimeAsync(5000);
    expect(transport.invoke).toHaveBeenCalledTimes(calls);
    unsubscribe();
  });
  it('rejects malformed progress and never leaks native transport errors', async () => {
    const bridge = await import('./update-bridge');
    expect(() => bridge.validateUpdateState({ ...state(1, 'ready'), downloaded: -1 })).toThrow(
      bridge.updateText.notices.requestFailed,
    );
    transport.invoke.mockRejectedValue('/private/token');
    await expect(bridge.installUpdate()).rejects.toThrow(bridge.updateText.notices.requestFailed);
    expect(transport.invoke).toHaveBeenCalledWith('install_update', { confirmed: true });
  });
  it('has an explicit English fallback for updater product copy', async () => {
    const bridge = await import('./update-bridge');
    expect(bridge.updateText.check).toBe('Check for updates');
    expect(bridge.updateText.notices.verificationFailed).toContain('It was not installed');
  });
});
