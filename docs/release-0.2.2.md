# 0.2.2 — сохранение квот между запусками

Две подтверждённые причины сброса: Coordinator хранил снимки и подключения
только в памяти, а правило свежести скрывало проценты через пять минут.
Регрессии до исправления: `last_quota_and_connection_survive_restart_without_running_a_cli`
получал0 окон после перезапуска вместо1; TS-тест15-минутного снимка получал
`C — · X —` вместо исторических процентов.

- `quota-cache.json` хранит только нормализованные снимки и список подключений.
  Запись после завершения чтения атомарная с правами0600. Ошибки записи
  видны в панели. Настройки, raw output, credentials и in-flight jobs не сохраняются.
- Чтение ограничено64KiB, проверяет версию, даты, проценты, окна и дубликаты.
  Повреждённый файл не меняется при загрузке, в UI появляется сообщение.
- Перезапуск восстанавливает снимок с исходным timestamp как историю.
  CLI не запускается во время восстановления. Выбранный таймер продолжает
  работать для сохранённых подключений после выбранного интервала.
- Проценты не исчезают при старении, обновлении или ошибке сервиса.
  История отмечена`~` и приглушена в меню, в карточке есть«по снимку» и возраст.
  Правило свежести для уведомлений сохранено; кэш сам не вызывает уведомлений.
  Прошедшее время сброса не превращается в новый лимит.
- Сроки паузы Claude продолжают использовать quota-retry.json. Активная пауза
  из0.2.1 восстанавливает ранее подключённого провайдера при миграции.
- Smoke `live_read --live [claude] --persist` явно включает app-owned cache.
  Default tests не запускают реальные CLI и не делают сетевых запросов.

Проверки: restart без CLI, failure→restart→history, возобновление timer,
ошибка записи, приватность/границы/повреждённый cache, UI age→reopen,
history при429 и реальная native растеризация. Подпись/установка — после сборки.

## Проверено и установлено

110 library + 2 examples + 46 TS + 15 Playwright = **173 cases**.
Clippy-Dwarnings, fmt, TS, Biome и production build пройдены.

Zamok подписал и нотарифицировал DMG. codesign--deep--strict, stapler и
Gatekeeper для consumer DMG/app пройдены (Notarized Developer ID).
DMG3 488 763 байта, SHA-256
`90eaebef2a8f97823b5f05f0e4f352db4f004e18ac8c9ed3dfd3fc478c7ea62b`.

0.2.2 установлена в /Applications/KnowYourLimits.app. Предыдущая версия закрыта
через owned SIGTERM/awaited shutdown и сохранена в
`/Users/akira/Library/Caches/KnowYourLimits/app-backups/KnowYourLimits-before-0.2.2-1791663195225.app`.

Получен настоящий Codex snapshot через production reader с явным--persist:
91%использовано, fetchedAt1791663165562. Cache restoration через настоящий
Coordinator восстановила`C — · X ~9%`, исходный timestamp и оба подключения,
не исполняя CLI и не меняя файл кэша. Settings checksum до/после установки
одинаков: `c6fa2631291f8020d9f2466440b290caa7c199ca82d73dcc31daac17a1e5c7b3`.
Claude quota GET в этой работе не выполнялся: сохранена прежняя пауза.

Приложение запущено через CUA, PID20023 активен. Native AX capture вернул
timeout; физическое окно не подтверждено. UI проверен Playwright с mock IPC,
растеризация — настоящими CoreText/CoreGraphics; cache — production Coordinator.
Временный cache probe удалён после проверки.

Скачивание: https://bshk--55999.getbb.app/KnowYourLimits-0.2.2.dmg;
порт55999, session70146. Consumer download hash совпадает с DMG,
старые ссылкиredirect на0.2.2.
