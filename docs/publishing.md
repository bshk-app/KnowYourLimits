# Публикация KnowYourLimits

Репозиторий: [bshk-app/KnowYourLimits](https://github.com/bshk-app/KnowYourLimits).
Windows 10/11 x64 публикуется через `.github/workflows/release.yml`.
macOS 13+ на Apple Silicon проверяется в CI; подписанный DMG пока собирается
отдельно на Mac с Developer ID, Zamok и доступом к нотаризации Apple.

## Релиз через GitHub Actions

Версия должна совпадать в `package.json`, `src-tauri/tauri.conf.json`
и `src-tauri/Cargo.toml`. Заметки находятся в `docs/releases/<version>.md`.
Изменения исходников после опубликованного релиза требуют новой версии.

```sh
bun run check:release
bun run test:release
git add .
git commit -m "Prepare release"
git push origin main
git tag -a v0.2.6 -m "KnowYourLimits 0.2.6"
git push origin v0.2.6
```

Для следующей версии замените номер во всех файлах и в теге.
Пуш тега `v*` запускает Release. Также можно вручную запустить этот workflow
из вкладки Actions, указав уже существующий тег. Произвольная ветка вместо
тега, несовпадение версии или тега и исходников останавливают публикацию.

Workflow переиспользует Check: сканирование всей истории на секреты,
TypeScript/Biome, frontend и release tests, Rust tests/fmt/Clippy на Windows
и macOS, Playwright на macOS, сборка Windows NSIS и macOS `.app`.
Ключи авторизации CLI и Apple для этих проверок не нужны. Git хранит текстовые
файлы с LF, в том числе при checkout на Windows.

После успешных проверок отдельное задание с `contents: write` получает
Windows-артефакты того же запуска. Оно сверяет версию, тег, commit сборки,
два ожидаемых имени файлов, размеры и SHA-256. Релиз создаётся черновиком,
файлы загружаются, затем проверяется их наличие и релиз публикуется.
Опубликованная версия не перезаписывается; незавершённый черновик можно
достроить повторным запуском workflow для того же тега.

Публикуются четыре файла:

- `KnowYourLimits-<version>-windows-x64-setup.exe` — установщик NSIS.
- `KnowYourLimits-<version>-windows-x64.exe` — запуск без установки.
- `KnowYourLimits-<version>-windows-x64-manifest.json` — версия, commit,
  размеры, SHA-256 и состояние подписи.
- `SHA256SUMS.txt` — SHA-256 установщика и отдельного EXE.

Установщик работает для текущего пользователя и при необходимости
устанавливает WebView2. Windows-сборка пока без подписи Authenticode.
Релиз не включает неподписанную macOS `.app`; DMG добавляется только после
Developer ID, успешной нотаризации и stapling.

## Локальная сборка Windows

```powershell
bun install --frozen-lockfile
bun run package:windows
```

Файлы появляются в `artifacts/windows/<version>/` вместе с `manifest.json`.
Локальные сборки разрешены с незакоммиченными изменениями, но такой manifest
содержит `sourceDirty: true` и не принимается публикацией.
Для публичного релиза используются файлы, собранные в GitHub Actions из тега.

## Исходники и секреты

В репозитории находятся исходники, тесты, документация, иконки, конфигурации
и lock-файлы. `.gitignore` исключает зависимости, сборки, `.env`, приватные
ключи, agent-настройки и файлы состояния приложения. Не используйте
`git add -f` для этих файлов. Автоматическое сканирование секретов использует
Gitleaks с закреплённой версией и проверенным SHA-256 бинарника.

Приложение не читает файлы credentials/cookies. Авторизацию выполняют
официальные Claude Code и Codex CLI на компьютере пользователя. Установщик
не содержит настройки, квоты или авторизацию машины, на которой собран.

История коммитов содержит обычные данные автора Git. Публичные fingerprint
Developer ID, Team ID и имя профиля Keychain не являются приватными ключами.
Их наличие не даёт доступа к сертификату или Apple аккаунту.

## Подписанный macOS DMG

Для локальной упаковки нужны `zamokctl`, Developer ID в Keychain и доступный
профиль `notarize` либо App Store Connect API key. Приватные ключи остаются
в Keychain или вне репозитория. Для другого профиля можно задать
`KYL_NOTARY_PROFILE`; fingerprint сертификата переопределяется через
`KYL_SIGNING_IDENTITY_SHA1`.

```sh
xcrun notarytool store-credentials notarize
bun run package:macos
```

Tauri собирает `.app`, затем Zamok подписывает, нотарифицирует и прикрепляет
билеты Apple к приложению и DMG. Скрипт проверяет manifest, SHA-256, `codesign`,
`stapler` и Gatekeeper. Артефакты находятся в `artifacts/macos/<version>/`.
Профиль `notarize` необходимо создать на выпускающем Mac; в репозитории нет
его credentials.

App Store Connect использует `APPLE_API_KEY_PATH`, `APPLE_API_KEY` и
`APPLE_API_ISSUER`, заданные вместе. Значения не добавляются в Git.
Параметры API не нужны для Windows-релиза.

Существующий `bun run release:github` — отдельный локальный путь публикации
подписанного DMG через Zamok. Он дополнительно требует выбранную `LICENSE`,
чистый Git, `KYL_GITHUB_REPOSITORY` и авторизацию `gh`. Windows-релиз уже
опубликованного тега добавлением другого workflow не заменяется.

Ссылки: [Tauri — сборка в GitHub](https://v2.tauri.app/distribute/pipelines/github/),
[GitHub CLI — создание релиза](https://cli.github.com/manual/gh_release_create),
[Tauri — подпись macOS](https://v2.tauri.app/distribute/sign/macos/).
