import { isNative } from './bridge';
import { node } from './dom';
import {
  cancelUpdateDownload,
  checkForUpdates,
  downloadUpdate,
  getUpdateState,
  installUpdate,
  subscribeUpdates,
  updateText as text,
  type UpdateState,
} from './update-bridge';

const format = (template: string, values: Record<string, string | number>) =>
  template.replace(/\{(\w+)\}/g, (match, name: string) => String(values[name] ?? match));

export function mountUpdates(parent: HTMLElement, onInstalling: (installing: boolean) => void) {
  if (!isNative) return;
  const section = node('section', 'kyl-updates');
  section.setAttribute('aria-label', text.title);
  const status = node('p', 'kyl-update-status');
  status.id = 'update-status';
  status.setAttribute('role', 'status');
  status.setAttribute('aria-live', 'polite');
  const progress = node('progress', 'kyl-update-progress');
  progress.setAttribute('aria-label', text.downloading);
  progress.max = 100;
  progress.hidden = true;
  const actions = node('div', 'kyl-update-actions');
  const check = node('button', 'kyl-btn text', text.check);
  check.id = 'check-for-updates';
  check.type = 'button';
  const action = node('button', 'kyl-btn');
  action.id = 'update-action';
  action.type = 'button';
  action.hidden = true;
  const confirmation = node('div', 'kyl-update-confirm');
  confirmation.setAttribute('role', 'group');
  confirmation.setAttribute('aria-label', text.install);
  confirmation.hidden = true;
  const explanation = node('p', '');
  explanation.id = 'update-confirmation';
  confirmation.setAttribute('aria-describedby', explanation.id);
  const confirm = node('button', 'kyl-btn primary', text.confirmInstall);
  confirm.id = 'confirm-install-update';
  confirm.type = 'button';
  const cancel = node('button', 'kyl-btn', text.cancel);
  cancel.type = 'button';
  confirmation.append(explanation, confirm, cancel);
  actions.append(check, action);
  section.append(status, progress, actions, confirmation);
  parent.append(section);
  let state: UpdateState | undefined;
  let pending = false;
  let error: string | null = null;

  function render() {
    const busy =
      pending ||
      (!error &&
        (state?.status === 'checking' ||
          state?.status === 'downloading' ||
          state?.status === 'installing'));
    check.disabled = Boolean(busy || (state && !state.supported));
    action.disabled = pending;
    confirm.disabled = pending;
    cancel.disabled = pending;
    section.setAttribute('aria-busy', String(Boolean(busy)));
    onInstalling(!error && state?.status === 'installing');
    action.hidden = true;
    progress.hidden = state?.status !== 'downloading';
    const version = state?.version ?? '';
    let label = '';
    if (state) {
      switch (state.status) {
        case 'checking':
          label = text.checking;
          break;
        case 'upToDate':
          label = text.upToDate;
          break;
        case 'available':
          label = format(text.available, { version });
          break;
        case 'downloading':
          if (state.total !== null) {
            const percent = Math.min(100, Math.floor((state.downloaded / state.total) * 100));
            progress.value = percent;
            label = format(text.downloadProgress, { percent });
          } else {
            progress.removeAttribute('value');
            label = text.downloading;
          }
          action.textContent = text.cancelDownload;
          action.hidden = false;
          break;
        case 'ready':
          label = format(text.ready, { version });
          break;
        case 'installing':
          label = text.installing;
          break;
      }
      if (state.installation !== 'managed') {
        label = [label, state.installation === 'development' ? text.development : text.manual]
          .filter(Boolean)
          .join(' ');
      } else if (state.status === 'available' || state.status === 'ready') {
        action.textContent = state.status === 'ready' ? text.install : text.download;
        action.hidden = false;
      }
      if (!state.supported) label = text.unsupported;
      if (state.notice) label = [text.notices[state.notice], label].filter(Boolean).join(' ');
    }
    status.textContent = error ?? label;
    status.hidden = !status.textContent;
    if (state?.status !== 'ready') confirmation.hidden = true;
  }
  async function run(operation: () => Promise<UpdateState>) {
    if (pending) return;
    pending = true;
    error = null;
    render();
    try {
      state = await operation();
    } catch (e) {
      error = e instanceof Error ? e.message : text.notices.requestFailed;
    } finally {
      pending = false;
      render();
    }
  }
  check.addEventListener('click', () => {
    confirmation.hidden = true;
    void run(checkForUpdates);
  });
  action.addEventListener('click', () => {
    if (state?.status === 'ready') {
      explanation.textContent = format(text.confirm, { version: state.version ?? '' });
      confirmation.hidden = false;
      confirm.focus();
    } else if (state?.status === 'downloading') void run(cancelUpdateDownload);
    else if (state?.status === 'available') void run(downloadUpdate);
  });
  cancel.addEventListener('click', () => {
    confirmation.hidden = true;
    action.focus();
  });
  confirm.addEventListener('click', () => {
    confirmation.hidden = true;
    void run(installUpdate);
  });
  subscribeUpdates(
    (next) => {
      state = next;
      error = null;
      render();
    },
    (notice) => {
      error = notice;
      render();
    },
  );
  // Restoring startup status is passive; failures never interrupt quota use.
  void getUpdateState().then(
    (next) => {
      state = next;
      render();
    },
    () => {},
  );
  window.addEventListener('focus', () => {
    void getUpdateState().catch(() => {});
  });
  render();
}
