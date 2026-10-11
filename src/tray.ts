import {
  type AppState,
  displayRemainingPercent,
  PROVIDER_SHORT,
  type ProviderId,
  remainingPercent,
  type TrayLayout,
} from './model';

export function providerSymbol(id: ProviderId): SVGElement {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 20 20');
  svg.setAttribute('aria-hidden', 'true');
  svg.classList.add('provider-symbol');
  if (id === 'claude') {
    for (let i = 0; i < 12; i++) {
      const a = (i * Math.PI) / 6;
      const line = document.createElementNS(svg.namespaceURI, 'line');
      line.setAttribute('x1', String(10 + Math.cos(a) * 3));
      line.setAttribute('y1', String(10 + Math.sin(a) * 3));
      line.setAttribute('x2', String(10 + Math.cos(a) * 9));
      line.setAttribute('y2', String(10 + Math.sin(a) * 9));
      line.setAttribute('stroke', 'currentColor');
      line.setAttribute('stroke-width', '2');
      line.setAttribute('stroke-linecap', 'round');
      svg.append(line);
    }
  } else {
    const path = document.createElementNS(svg.namespaceURI, 'path');
    path.setAttribute('d', 'M2 4L9 10L2 16M12 16H19');
    path.setAttribute('fill', 'none');
    path.setAttribute('stroke', 'currentColor');
    path.setAttribute('stroke-width', '2.2');
    path.setAttribute('stroke-linecap', 'round');
    path.setAttribute('stroke-linejoin', 'round');
    svg.append(path);
  }
  return svg;
}
export function renderTray(
  container: HTMLElement,
  state: AppState,
  layout: TrayLayout = state.settings.layout,
) {
  container.classList.add('tray-visual');
  for (const candidate of ['inline', 'stacked', 'rings'])
    container.classList.remove(`layout-${candidate}`);
  container.classList.add(`layout-${layout}`);
  container.replaceChildren();
  for (const id of state.settings.barProviders) {
    const preferences = state.settings.preferences[id];
    const provider = state.providers.find((p) => p.id === id);
    const remaining = provider ? displayRemainingPercent(provider) : null;
    const historical = state.refreshing === id || !provider || remainingPercent(provider) === null;
    const column = document.createElement('span');
    column.className = `tray-provider size-${preferences.size}`;
    column.style.color = preferences.color === 'system' ? 'var(--text)' : preferences.color;
    column.classList.toggle('no-quota', remaining === null);
    column.classList.toggle('historical', remaining !== null && historical);
    const mark = document.createElement('span');
    mark.className = `tray-mark icon-${preferences.icon}`;
    if (preferences.icon === 'symbol') mark.append(providerSymbol(id));
    else if (preferences.icon === 'letter') mark.textContent = PROVIDER_SHORT[id];
    else if (preferences.icon === 'dot') mark.append(document.createElement('i'));
    else if (layout === 'stacked') {
      mark.classList.add('text-label');
      mark.textContent = id === 'claude' ? 'CLA' : 'CDX';
    }
    const value = document.createElement('span');
    value.className = 'tray-value';
    value.textContent =
      remaining === null ? '—' : `${historical ? '~' : ''}${Math.round(remaining)}%`;
    if (layout === 'rings') {
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.setAttribute('viewBox', '0 0 22 22');
      svg.classList.add('tray-ring');
      for (const active of [false, true]) {
        if (active && (remaining === null || remaining === 0)) continue;
        const circle = document.createElementNS(svg.namespaceURI, 'circle');
        circle.setAttribute('cx', '11');
        circle.setAttribute('cy', '11');
        circle.setAttribute('r', '8.5');
        circle.setAttribute('fill', 'none');
        circle.setAttribute('stroke', 'currentColor');
        circle.setAttribute('stroke-width', '1.8');
        if (active) {
          circle.setAttribute('pathLength', '100');
          circle.setAttribute('stroke-dasharray', `${remaining} 100`);
          circle.setAttribute('transform', 'rotate(-90 11 11)');
          circle.setAttribute('stroke-linecap', 'round');
        } else circle.setAttribute('opacity', '0.22');
        svg.append(circle);
      }
      column.append(svg, mark);
    } else column.append(mark, value);
    container.append(column);
  }
}
