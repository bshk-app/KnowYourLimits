# Проверка прототипа P0

## Выполнено

- `bun run test`: **34 unit tests** (модель квот и preview settings bridge).
- `cargo test`: **13 tests** (native модель, camelCase DTO и atomic settings storage).
- `bun run test:e2e`: **6 Chromium tests** — оба провайдера, минимальный остаток,
  critical/stale/error/unavailable, сохранение после reload, draft/focus при таймерах,
  истечение свежести, validation, phone/dark layout и отсутствие внешних account requests.
- TypeScript strict check, Biome check, cargo fmt и clippy `-D warnings`: прошли.
- Vite production build: прошёл; JS ~18 KB, CSS ~8 KB до gzip (не размер приложения).
- Tauri debug bundle: `src-tauri/target/debug/bundle/macos/KnowYourLimits.app`, ~25.31 MiB.
  Это debug `.app`, не оптимизированный release и не notarized public distribution.
- Финальный packaged app запущен: finishedLaunching, ActivationPolicy::Accessory,
  один процесс. Smoke-test процесс остановлен после проверки.
- Повторный launch в smoke предыдущей сборки оставлял один экземпляр после завершения
  инициализации; моментальный probe мог наблюдать второй краткоживущий процесс.
- Preview сервер на 4173: HTTP 200 локально и с Host удалённой BB share-ссылки.

## Ревью и намеренное уточнение поведения

Независимое ревью выявило race у visibility-based toggle: macOS focus loss может скрыть
панель до tray mouse-up, после чего callback ошибочно открывает её снова.
P0 намеренно упрощён: **левый клик всегда открывает панель**, без toggle;
Escape / потеря фокуса / закрытие скрывают её. Изменение записано как REVISED в
[спецификации](specs/prototype.md). Повторная проверка reviewer не нашла ошибки
в этом минимальном изменении. Это не доказательство отсутствия всех багов.

## Что не проверено полностью

- Физические клики по native tray, визуальная фокусировка, clipboard, multi-monitor/DPI
  и границы панели требуют ручной macOS QA. Браузерные tests не заменяют её.
- Screen Recording permission отсутствует; native screenshots не снимались, разрешения
  не запрашивались и не менялись. Browser screenshot: `artifacts/prototype-dark.png`.
- Настройки native storage проверены unit tests; реальные WebView IPC/actions в packaged
  `.app` пока не проверены end-to-end.
- Notifications/autostart, real CLI adapters, live polling и restart-safe alert dedup
  НЕ реализованы. Все проценты вымышлены, app/tray/UI обозначены DEMO.
- macOS 13 — configured minimum, не утверждение о тестировании на всех версиях macOS.

## Просмотр

Для локальной проверки preview запускается командой `bun run preview`.
Сервер — background task `kyl-prototype-preview`; прекращение/перезагрузка сессии может
остановить его. Для повторного запуска используйте `bun run build && bun run preview`.
