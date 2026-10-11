import { localizeDOM, localizeNotice, percent, setLanguage, t, text, windowLabel } from './i18n';
import './style.css';
import appIcon from '../assets/icons/app.svg';
import {
  cancelRead,
  fitPanel,
  getState,
  hidePanel,
  isNative,
  openSettings,
  readQuota,
  type SettingsTab,
  signInClaude,
  subscribeState,
} from './bridge';
import { element, message, node } from './dom';
import {
  type AppState,
  defaultSettings,
  displayRemainingPercent,
  formatAge,
  formatReset,
  formatRetry,
  isProviderPaused,
  PROVIDER_IDS,
  PROVIDER_LABELS,
  PROVIDER_SHORT,
  type ProviderId,
  type ProviderSnapshot,
  type QuotaWindow,
  quotaTone,
  remainingPercent,
  trayTitle,
} from './model';
import { providerSymbol, renderTray } from './tray';

const app = element('#app');
app.innerHTML = `
  <aside class="preview-frame" data-i18n-aria="menuBar">
    <span class="preview-label">${text('preview')}</span>
    <div class="menu-bar"><span id="tray-visual" aria-hidden="true"></span>
      <output id="tray-preview" class="sr-only" data-i18n-aria="remainingMenu">—</output>
    </div>
  </aside>
  <section class="kyl-panel" aria-labelledby="app-title">
    <header class="kyl-head">
      <img src="${appIcon}" alt="">
      <div><h1 id="app-title">KnowYourLimits</h1><p id="subtitle">${text('subtitle')}</p></div>
    </header>
    <div class="kyl-body">
      <p id="bridge-notice" class="kyl-callout" role="status" aria-live="polite" hidden></p>
      <p id="loading" class="kyl-note" role="status">${text('loading')}</p>
      <div id="fatal" class="kyl-callout crit" hidden>
        <h2>${text('loadFailed')}</h2>
        <p id="fatal-message"></p>
        <div><button id="retry-load" class="kyl-btn" type="button">${text('retry')}</button></div>
      </div>
      <div id="provider-cards" class="kyl-cards" data-i18n-aria="quotas" hidden></div>
      <p id="browser-live-hint" class="kyl-note">${text('desktopOnly')}</p>
    </div>
    <div class="kyl-menu"><button id="open-settings" class="kyl-menu-item" type="button">${text('settings')}<kbd aria-hidden="true">⌘,</kbd></button></div>
  </section>
`;
localizeDOM();
document.body.classList.add(isNative ? 'native' : 'preview');

const loading = element('#loading');
const fatal = element('#fatal');
const notice = element('#bridge-notice');
const tray = element('#tray-preview');
const trayVisual = element('#tray-visual');
const cardsContainer = element('#provider-cards');
const retryButton = element<HTMLButtonElement>('#retry-load');
element('#browser-live-hint').hidden = isNative;

// The panel window follows its content like a popover; a failed resize keeps the old size.
const panel = element('.kyl-panel');
new ResizeObserver(() => {
  fitPanel(panel.offsetHeight).catch(() => {});
}).observe(panel);

interface WindowView {
  root: HTMLElement;
  label: HTMLElement;
  used: HTMLElement;
  bar: HTMLElement;
  fill: HTMLElement;
  reset: HTMLElement;
  status: HTMLElement;
}
interface CardView {
  root: HTMLElement;
  tile: HTMLElement;
  status: HTMLElement;
  left: HTMLElement;
  leftValue: HTMLElement;
  leftLabel: HTMLElement;
  text: HTMLElement;
  windows: HTMLElement;
  rows: Map<string, WindowView>;
  action: HTMLButtonElement;
  foot: HTMLElement;
  age: HTMLElement;
}

let state: AppState | null = null;
let busy = false;
let pendingCommands = 0;
let focusPending = false;
let hidePending = false;
let commandError: string | null = null;
let subscriptionNotice: string | null = null;

function showSettings(tab: SettingsTab) {
  openSettings(tab).catch((error) => {
    commandError = message(error);
    renderNotice();
  });
}

const cards = new Map<ProviderId, CardView>();
for (const id of PROVIDER_IDS) {
  const root = node('article', 'kyl-card');
  root.setAttribute('aria-label', PROVIDER_LABELS[id]);
  const head = node('header', 'kyl-card-head');
  const tile = node('span', 'kyl-tile');
  tile.setAttribute('aria-hidden', 'true');
  const identity = node('div', '');
  const status = node('div', 'kyl-status');
  identity.append(node('h2', 'kyl-name', PROVIDER_LABELS[id]), status);
  const left = node('div', 'kyl-left');
  const leftValue = node('b', '', '—');
  const leftLabel = node('span', '', t('remaining'));
  left.append(leftValue, leftLabel);
  const gear = node('button', 'kyl-icon-btn');
  gear.type = 'button';
  gear.setAttribute('aria-label', t('settingsProvider', { provider: PROVIDER_LABELS[id] }));
  gear.addEventListener('click', () => showSettings(id));
  head.append(tile, identity, left, gear);
  const text = node('p', 'kyl-card-text');
  const windows = node('div', 'kyl-windows');
  const action = node('button', 'kyl-btn');
  action.id = `connect-${id}`;
  action.type = 'button';
  action.addEventListener('click', () => {
    if (!isNative || !state) return;
    // While this provider reads, the same button cancels the read.
    if (state.refreshing === id) {
      void command(t('errorCancel'), cancelRead, undefined, true);
      return;
    }
    if (state.refreshing !== null || busy || isProviderPaused(state, id)) return;
    const signedOut = state.providers.find((p) => p.id === id)?.status === 'signedout';
    void command(t('errorConnect', { provider: PROVIDER_LABELS[id] }), () =>
      id === 'claude' && signedOut ? signInClaude() : readQuota(id),
    );
  });
  const foot = node('footer', 'kyl-card-foot');
  const age = node('span', '');
  foot.append(age);
  root.append(head, text, windows, action, foot);
  cardsContainer.append(root);
  cards.set(id, {
    root,
    tile,
    status,
    left,
    leftValue,
    leftLabel,
    text,
    windows,
    rows: new Map(),
    action,
    foot,
    age,
  });
}

function createWindow(view: CardView): WindowView {
  const root = node('div', 'kyl-window');
  const heading = node('div', 'kyl-window-row');
  const label = node('b', '');
  const used = node('span', 'used');
  heading.append(label, used);
  const bar = node('div', 'kyl-bar');
  bar.setAttribute('role', 'meter');
  bar.setAttribute('aria-valuemin', '0');
  bar.setAttribute('aria-valuemax', '100');
  const fill = node('i', '');
  bar.append(fill);
  const detail = node('div', 'kyl-window-row small');
  const reset = node('span', '');
  const status = node('span', 'tone');
  detail.append(reset, status);
  root.append(heading, bar, detail);
  view.windows.append(root);
  return { root, label, used, bar, fill, reset, status };
}

type Tone = ReturnType<typeof quotaTone>;
const TONE_ORDER: Tone[] = ['normal', 'warning', 'critical', 'exhausted'];

function validPercent(quota: QuotaWindow) {
  return Number.isFinite(quota.usedPercent) && quota.usedPercent >= 0 && quota.usedPercent <= 100;
}

function renderWindow(
  row: WindowView,
  quota: QuotaWindow,
  fresh: boolean,
  thresholds: number[],
  now: number,
) {
  const valid = validPercent(quota);
  const expired = quota.resetsAt !== null && quota.resetsAt <= now;
  const historical = !fresh || expired;
  const tone = valid ? quotaTone(quota.usedPercent, thresholds) : 'normal';
  row.root.className = `kyl-window${historical ? ' historical' : tone === 'normal' ? '' : ` ${tone}`}`;
  row.label.textContent = windowLabel(quota);
  row.used.textContent = valid ? t('used', { percent: percent(quota.usedPercent) }) : t('noData');
  row.bar.hidden = !valid;
  row.bar.setAttribute('aria-valuenow', String(valid ? Math.round(quota.usedPercent) : 0));
  row.bar.setAttribute(
    'aria-label',
    `${windowLabel(quota)}: ${row.used.textContent}, ${t(historical ? 'pastValue' : 'currentValue')}`,
  );
  row.fill.style.setProperty('--v', `${valid ? quota.usedPercent : 0}%`);
  const ticks = thresholds
    .filter((threshold) => threshold < 100)
    .map((threshold) => {
      const tick = document.createElement('s');
      tick.style.setProperty('--t', `${threshold}%`);
      return tick;
    });
  row.bar.replaceChildren(row.fill, ...ticks);
  const reset = formatReset(quota.resetsAt, now);
  row.reset.textContent = reset;
  row.status.replaceChildren();
  if (!valid) row.status.textContent = t('noData');
  else if (!historical && tone !== 'normal') {
    const glyph = node('span', 'kyl-glyph');
    glyph.setAttribute('aria-hidden', 'true');
    row.status.append(glyph, ` ${t(tone)}`);
  }
}

function badge(text: string, tone = '') {
  return node('span', `kyl-badge${tone ? ` ${tone}` : ''}`, text);
}

function renderTile(view: CardView, id: ProviderId) {
  const preferences = state?.settings.preferences[id] ?? defaultSettings().preferences[id];
  // The tile repeats the menu-bar icon; without one it keeps the provider symbol.
  view.root.style.setProperty(
    '--provider',
    preferences.color === 'system' ? '' : preferences.color,
  );
  if (preferences.icon === 'letter') view.tile.replaceChildren(PROVIDER_SHORT[id]);
  else if (preferences.icon === 'dot') view.tile.replaceChildren(node('i', 'dot'));
  else view.tile.replaceChildren(providerSymbol(id));
}

function renderProvider(
  view: CardView,
  provider: ProviderSnapshot | undefined,
  id: ProviderId,
  now: number,
) {
  renderTile(view, id);
  const thresholds = state?.settings.thresholds[id] ?? defaultSettings().thresholds[id];
  const disconnected = !state?.connected.includes(id);
  const refreshing = state?.refreshing === id;
  const signingIn = refreshing && !!state?.signingIn;
  const signedOut = provider?.status === 'signedout';
  const paused = state ? isProviderPaused(state, id, now) : false;
  const remaining = !disconnected && provider ? displayRemainingPercent(provider, now) : null;
  const fresh =
    !disconnected && !refreshing && !!provider && remainingPercent(provider, now) !== null;
  const windows = disconnected ? [] : (provider?.windows ?? []);
  // Expired windows are past values; they must not color the live remaining number.
  const worst = fresh
    ? windows
        .filter(
          (quota) => validPercent(quota) && !(quota.resetsAt !== null && quota.resetsAt <= now),
        )
        .map((quota) => quotaTone(quota.usedPercent, thresholds))
        .reduce<Tone>((a, b) => (TONE_ORDER.indexOf(b) > TONE_ORDER.indexOf(a) ? b : a), 'normal')
    : 'normal';
  // Connection states show a full-width button instead of numbers and a footer.
  const connecting = disconnected || signedOut || signingIn;

  view.status.replaceChildren(
    ...(paused
      ? [badge(t('paused'), 'warn')]
      : signingIn
        ? [node('span', 'kyl-dot'), t('signingIn')]
        : refreshing
          ? [node('span', 'kyl-dot'), t('refreshing')]
          : signedOut
            ? [badge(t('signedOut'), 'warn')]
            : disconnected
              ? [t('disconnected')]
              : !provider || provider.status === 'unavailable'
                ? [badge(t('unavailable'))]
                : provider.status === 'error'
                  ? [badge(t('refreshFailed'), 'crit')]
                  : !fresh
                    ? [badge(t('outdated'))]
                    : worst === 'exhausted'
                      ? [badge(t('exhausted'), 'crit')]
                      : worst === 'critical'
                        ? [badge(t('critical'), 'crit')]
                        : worst === 'warning'
                          ? [badge(t('warning'), 'warn')]
                          : [badge(t('current'), 'ok')]),
  );

  view.left.hidden = connecting;
  view.leftValue.textContent = remaining === null ? '—' : percent(remaining);
  view.leftLabel.textContent = t(remaining === null ? 'noData' : 'remaining');
  view.left.className = `kyl-left${
    remaining === null
      ? ' is-empty'
      : !fresh
        ? ' is-past'
        : worst === 'warning'
          ? ' warn'
          : worst === 'normal'
            ? ''
            : ' crit'
  }`;

  view.root
    .querySelector('button.kyl-icon-btn')
    ?.setAttribute('aria-label', t('settingsProvider', { provider: PROVIDER_LABELS[id] }));
  view.text.textContent = paused
    ? t('rateLimited')
    : signingIn
      ? t('finishSignIn')
      : signedOut
        ? t('signInHelp')
        : disconnected
          ? t('connectHelp')
          : refreshing
            ? ''
            : windows.length === 0
              ? t('noQuota')
              : '';
  view.text.hidden = !view.text.textContent;

  const currentIds = new Set(windows.map((quota) => quota.id));
  for (const [key, row] of view.rows) {
    if (!currentIds.has(key)) {
      row.root.remove();
      view.rows.delete(key);
    }
  }
  for (const quota of windows) {
    let row = view.rows.get(quota.id);
    if (!row) {
      row = createWindow(view);
      view.rows.set(quota.id, row);
    }
    renderWindow(row, quota, fresh, thresholds, now);
  }
  view.windows.hidden = windows.length === 0;

  const label = PROVIDER_LABELS[id];
  view.action.className = connecting
    ? `kyl-btn block${signingIn ? '' : ' primary'}`
    : 'kyl-btn text';
  view.action.textContent =
    paused && state?.retryAt[id] != null
      ? formatRetry(state.retryAt[id], now)
      : signingIn
        ? t('cancelSignIn')
        : refreshing
          ? t('cancelRefresh')
          : signedOut
            ? t('signInClaude')
            : disconnected
              ? t('connect', { provider: label })
              : t('refresh');
  if (view.action.textContent === t('refresh'))
    view.action.setAttribute('aria-label', t('refreshProvider', { provider: label }));
  else view.action.removeAttribute('aria-label');
  // Moving a node drops its focus, so the button changes place only with the card's state.
  if (connecting && view.action.parentElement !== view.root)
    view.root.insertBefore(view.action, view.foot);
  if (!connecting && view.action.parentElement !== view.foot) view.foot.append(view.action);

  view.foot.hidden = connecting;
  const fetchedAt = disconnected ? null : (provider?.fetchedAt ?? null);
  view.age.textContent =
    fetchedAt === null ? t('noData') : t('updated', { time: formatAge(fetchedAt, now) });
}

// Timer updates only the read-only display.
function renderDisplay() {
  if (!state) return;
  const current = state;
  element('#subtitle').textContent = t('subtitle');
  const now = Date.now();
  tray.textContent = trayTitle(current, now);
  renderTray(trayVisual, current);
  for (const [id, view] of cards)
    renderProvider(
      view,
      current.providers.find((provider) => provider.id === id),
      id,
      now,
    );
}

function renderNotice() {
  notice.textContent = [
    subscriptionNotice ? localizeNotice(subscriptionNotice) : null,
    commandError,
    state?.notice ? localizeNotice(state.notice) : null,
  ]
    .filter(Boolean)
    .join(' ');
  notice.hidden = !notice.textContent;
}

function renderControls() {
  app.setAttribute('aria-busy', String(busy || state?.refreshing != null));
  for (const [id, view] of cards)
    view.action.disabled =
      !isNative ||
      !state ||
      isProviderPaused(state, id) ||
      (state.refreshing === null ? busy : state.refreshing !== id);
  retryButton.disabled = busy;
}

function applyState(next: AppState) {
  if (state && next.revision <= state.revision) return;
  state = next;
  setLanguage(next.settings.language === 'system' ? next.systemLocale : next.settings.language);
  localizeDOM();
  cardsContainer.hidden = false;
  fatal.hidden = true;
  loading.hidden = true;
  renderDisplay();
  renderNotice();
  renderControls();
}

async function command(
  label: string,
  operation: () => Promise<AppState> | Promise<void>,
  success?: () => void,
  allowWhileBusy = false,
) {
  if (busy && !allowWhileBusy) return;
  // A passive focus read must not erase a failed mutation's notice.
  if (operation !== getState || !state) commandError = null;
  renderNotice();
  pendingCommands += 1;
  busy = true;
  renderControls();
  try {
    const next = await operation();
    if (next) applyState(next);
    success?.();
  } catch (error) {
    commandError = `${label}: ${message(error)}`;
    if (!state) {
      fatal.hidden = false;
      element('#fatal-message').textContent = commandError;
    }
  } finally {
    loading.hidden = true;
    renderNotice();
    pendingCommands -= 1;
    busy = pendingCommands > 0;
    renderControls();
    if (!busy && hidePending) {
      hidePending = false;
      focusPending = false;
      void command(t('errorHide'), hidePanel);
    } else if (!busy && focusPending) {
      focusPending = false;
      void command(t('errorLoad'), getState);
    }
  }
}

element('#open-settings').addEventListener('click', () => showSettings('menubar'));
retryButton.addEventListener('click', () => {
  loading.hidden = false;
  void command(t('errorLoad'), getState);
});
window.addEventListener('focus', () => {
  if (busy) focusPending = true;
  else void command(t('errorLoad'), getState);
});
window.addEventListener('keydown', (event) => {
  if (event.metaKey && event.key === ',') {
    event.preventDefault();
    showSettings('menubar');
  } else if (event.key === 'Escape' && isNative) {
    event.preventDefault();
    if (busy) hidePending = true;
    else void command(t('errorHide'), hidePanel);
  }
});
// The browser preview opens settings in a popup that saves to localStorage.
if (!isNative) window.addEventListener('storage', () => location.reload());
subscribeState(applyState, (text) => {
  subscriptionNotice = text;
  renderNotice();
});
window.setInterval(() => {
  renderDisplay();
  renderControls();
}, 5_000);
void command(t('errorLoad'), getState);
