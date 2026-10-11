# 0.1.2: Claude и удаление демо

Проблема: в 0.1.1 можно было подключить только Codex. Claude всегда показывал
«Не подключён», а панель предлагала возврат к вымышленным данным.

Теперь обе карточки предлагают собственное подключение и обновление.
В модели отсутствуют Mode, Scenario и генераторы демоснимков; удалён bundled
`src/fixtures/demo.json`. Демо удалено из IPC, меню иконки, браузерного интерфейса,
CSS и начального HTML. Синтетические данные используются только в тестах.

Состояние хранит список подключённых провайдеров и конкретный провайдер,
который сейчас обновляется. Чтение Claude сохраняет снимок Codex и наоборот.
Coordinator допускает один активный процесс, объединяет повторные одинаковые
действия, ожидает cleanup предыдущей задачи, сохраняет изменения настроек и
отсекает поздние результаты после отмены/выхода.

## Claude

Проверка использует тот же официальный CLI 2.1.296 и структурированный `/usage`,
который ранее находился только в standalone spike. Перед чтением выполняется
`claude auth status --json`; identity и сырые diagnostics не входят в DTO.

При первом чтении CLI сообщил `loggedIn:false`, `authMethod:none`;
позже тот же официальный auth status сообщил выполненный вход с подпиской.
Старая проверка `/usage` молча возвращала пустой снимок. Теперь отсутствие входа
даёт `signedout` и кнопку **«Войти в Claude»**.

Вход запускается только отдельным явным действием пользователя:
`claude auth login --claudeai`. Официальный CLI открывает браузер, проводит OAuth
и сохраняет авторизацию. Приложение отбрасывает вывод и принимает только
статическое сообщение успешного входа; затем повторно проверяет auth status
и читает квоту. Никакие credential-файлы приложение не открывает.
Вход ограничен 180 секундами и может быть отменён из панели или меню.
Отмена и выход ожидают завершения owned process group.

Окна Claude: global `session` / `weekly_all`, без token estimates, PTY, inference
или чтения текстовых процентов при отсутствии machine report. Источник может
вернуть пустые structured limits; это отображается как недоступная квота.
Пустой ответ источника по-прежнему не превращается в вымышленные проценты.
Browser sign-in покрыт синтетическими CLI/IPC тестами; реальный пользовательский
вход в браузере агентом не выполнялся.

### Исправление пустого cold read

Найдена причина source gap прежнего spike: в локальном Claude Code 2.1.296
`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` убирает structured `limits` из cold
`/usage`. Дифференциальная проверка одной переменной показала:

- флаг включён → один quota GET, без cache answer, успешный result, но нет окон;
- флаг отсутствует → один quota GET, без cache answer, global session/weekly окна;
- в обоих случаях нулевые turns, API duration, стоимость и model tokens.

Убирается только umbrella flag в окружении дочернего CLI, включая возможное
наследование из parent. Telemetry, error reporting, updates, background tasks,
memory, hooks, MCP и tools продолжают отключаться по отдельности.
Глобальное окружение не изменяется. Regression проверяет реальный subprocess
launch и сначала падал (`Command`), затем проходит после исправления.
Это наблюдение pinned CLI, а не утверждение о других версиях.

## Проверки

- Исходный UI repro: `bun run test:e2e tests/native.spec.ts -g 'Claude can be connected'`
  сначала упал из-за отсутствующей кнопки; после исправления проходит.
- 84 Rust library + 2 example + 41 TypeScript + 10 Playwright cases.
- Проверены реальный production resolver → managed Claude reader на синтетическом
  CLI, auth failures, официальный login dispatch, cleanup всех стадий,
  связь двух счётчиков, single-flight, ошибки/expired quota и поздние события.
- Реальный Codex smoke через production coordinator вернул текущую недельную квоту.
  Реальный Claude был проверен на пустом ответе, затем на настоящих session/weekly
  окнах после исправления окружения. Точные итоговые проверки дописываются ниже.
- Default tests не выполняют сетевую авторизацию. Browser тесты используют mock IPC;
  физическая проверка packaged WebView отмечается отдельно после сборки.

## Сборка

Версия обновлена до 0.1.2 в frontend, Cargo/lock и Tauri.
`bun run package:macos` использует тот же Zamok workflow, Developer ID
Aleksandr Beshkenadze (Q8H6GWJ658) и Keychain-профиль `notarize`.
Credentials не извлекаются. Публикация продукта на Zamok/GitHub не выполняется.
Артефакт: `artifacts/macos/0.1.2/KnowYourLimits-0.1.2.dmg`.
Финальный DMG: 3 395 199 байт, SHA-256
`3b8d6ebf961e27abf6d3517e5de01db793affa2ff70dd1bde9b490c62e3a2e5a`.
Apple notarization и stapling подтверждены. Gatekeeper принимает DMG и приложение
внутри смонтированного read-only consumer DMG; codesign --deep --strict проходит.

Исправленный production coordinator с минимальным PATH получил настоящие
session/weekly окна Claude без предварительного запуска диагностического probe.
Codex также вернул текущую недельную квоту. В дифференциальном CLI probe
подтверждён GET без cache answer и нулевые inference counters.

Финальная версия установлена в `/Applications/KnowYourLimits.app`.
Старая версия корректно завершена SIGTERM, сохранена резервная копия:
`~/Library/Caches/KnowYourLimits/app-backups/KnowYourLimits-before-0.1.2-1791656658488.app`.
У установленного bundle проверены версия 0.1.2 и строгая подпись; процесс запущен.
Computer Use при попытке захвата панели вернул timeoutReached, поэтому физический
клик подключения/входа и packaged WebView визуально не подтверждены. Проверки
реального ACL, production coordinator и UI с mock IPC проходят.

Финальный download server: порт 55999, сессия 96594. Старые ссылки 0.1.0/0.1.1
перенаправляются на финальную 0.1.2. Промежуточный artifact до исправления cold read
сохранён отдельно в `artifacts/macos/0.1.2-before-cold-read-fix` и не раздаётся.
