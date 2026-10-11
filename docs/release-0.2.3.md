# 0.2.3 — применение интервала автообновления

Пользователь выбрал15мин, но оба провайдера показывали «Ручное обновление»
и снимки старше4ч. App-owned settings.json подтверждал refreshMinutes=0 у обоих.
Причина: выбор в select менял только draft до отдельного submit кнопкой
«Сохранить», расположенной ниже остальных настроек.

Regression сначала RED: выбор15мин без submit оставлял schedule-status
«Ручное обновление». Теперь change select сразу сохраняет и применяет интервал.
Карточка показывает действующую настройку, её значение переживает закрытие
настроек и перезапуск. Ошибка записи возвращает select к действующему значению
и выводит сообщение рядом с полем и в панели.

Новый native command set_refresh_minutes атомарно изменяет только timer выбранного
провайдера, сохраняя остальные применённые настройки. Другие несохранённые
поля UI не применяются, permission уведомлений не запрашивается из timer change.
Пока запрос сохранения выполняется, оба select timer недоступны.
save_settings и partial update используют общий locked commit/persistence путь.
Дедупликация уведомлений не сбрасывается изменением timer.

Backend tests проверяют15мин до/после deadline, запуск после восстановления,
независимость preferences, invalid values и rollback при ошибке записи.
UI tests проверяют выбор без Save/close/reopen, несохранённые notification/size
drafts, save error rollback и отсутствие raw diagnostics.

## Результат проверки и установка

183 cases:116Rust library +2examples +47TS +18Playwright. fmt, Clippy-Dwarnings,
typecheck, Biome и production build пройдены. Regression выбора15мин RED→GREEN.

Zamok подписал и нотарифицировал0.2.3. codesign/deep/strict, Gatekeeper
(Notarized Developer ID) и stapler для DMG/app пройдены. DMG3 498 977байт,
SHA-256 `69acd3ffec367501a4e95629a0153cc1d6787d224cced0148d11bbfc89759090`.
Consumer download hash совпадает. Ссылка:
`artifacts/macos/0.2.3/KnowYourLimits-0.2.3.dmg`.

Установлено /Applications/KnowYourLimits.app. Старая0.2.2 завершена через
owned SIGTERM/awaited shutdown, backup:
`~/Library/Caches/KnowYourLimits/app-backups/KnowYourLimits-before-0.2.3-1791737291084.app`.
Сохранена исходная settings.json в том же каталоге с timestamp1791737291084.

Нативная0.2.3 запущена с закрытой панелью и временным интервалом1мин,
PID85042. Live smoke не запускался, кнопки чтения не нажимались.
Сам background ticker обновил оба источника с morning fetchedAt
Claude1791720096820/Codex1791720162026 до
Claude1791737358213/Codex1791737362877 (current, остаток67%/97%).
Следующий автоматический цикл обновил даты до
Claude1791737423128/Codex1791737427985, обаcurrent. Это два настоящих
фоновых цикла при минутном интервале. Default tests не запускают реальные CLI.

После smoke приложение завершено, обоим провайдерам атомарно сохранены15мин,
затем приложение запущено заново. Остальные настройки не изменены:
checksum с обнулёнными только refreshMinutes до/после совпал
`923da1a73e7e0a5884975b393ff50aa4671020a7421eaa821447fadf96dee5c5`.
15-минутный deadline проверен детерминированным тестом (899sне читает,
901sчитает), реальный15-минутный цикл в этом smoke не ожидался.
CUA AXcapture timeout; запуск подтверждён процессом, фоновые чтения —
изменением настоящих snapshots на диске. Физические клики UI не заявляются.
