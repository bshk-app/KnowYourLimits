# Публикация KnowYourLimits

Поддерживаются macOS 13+ на Apple Silicon и Windows 10/11 x64. Проверки
исходников и сборки выполняются в GitHub Actions (`.github/workflows/check.yml`).
Подписанный macOS-релиз собирается на Mac с Developer ID и доступом к нотаризации.
Windows-установщик NSIS собирается командой `bun run package:windows`; `.exe`
и `manifest.json` с SHA-256 находятся в `artifacts/windows/<version>/`.
Сборка Windows пока без подписи Authenticode.

## Как устроена сборка

Используется схема Tauri-версии Tish: Tauri создаёт `.app`, затем
`zamokctl package` подписывает, нотарифицирует и прикрепляет билеты Apple
к своей копии приложения и DMG. Проверяются `manifest.json`, версия,
идентификатор приложения, минимальная macOS, SHA-256, `codesign`, `stapler`
и Gatekeeper. Распространяется DMG из `artifacts/macos/<version>/`.

У KnowYourLimits нет Tauri updater. Для публикации на GitHub используется
`zamokctl github-release`; сервер Zamok, продукт, лицензирование Zamok
и ключи обновления Tish не нужны. Команда прикладывает DMG и `manifest.json`.

Версия должна совпадать в `package.json`, `src-tauri/tauri.conf.json`
и `src-tauri/Cargo.toml`. Публичные заметки находятся в
`docs/releases/<version>.md`. Проверка: `bun run check:release`.

## Что осталось перед первым релизом

- Исходники размещаются в приватном репозитории `bshk-app/KnowYourLimits`.
- Выбрать открытую лицензию и добавить `LICENSE`.
- Восстановить профиль нотаризации `notarize` либо указать существующий
  профиль или ключ App Store Connect. Developer ID из `packaging/macos.json`
  найден; профиль `notarize` отсутствует.
- Вручную проверить нативную панель, настройки, подключение CLI,
  отмену чтения и сохранение настроек после перезапуска.

До получения `notarized: true` и `stapled: true` в новом manifest macOS-сборка
0.2.4 не считается готовым скачиваемым релизом. Это не блокирует Windows-сборку.

## Исходники

Для публичного репозитория нужны исходники `src/`, `src-tauri/` (без `target/`
и `gen/`), `assets/`, `tests/`, `scripts/`, `packaging/`, `.github/`, `docs/`,
HTML, конфигурации, README, LICENSE и оба lock-файла.
`.gitignore` исключает зависимости, сборки, артефакты, `.env`, приватные
ключи, локальные настройки агентов, `claudedocs/` и рабочий `progress.md`.
Не используйте `git add -f` для этих файлов.

После выбора лицензии и подтверждения адреса:

```sh
git init -b main
git add .
git diff --cached --stat
gitleaks git --staged --redact
git commit -m "Initial open-source release preparation"
gh repo create OWNER/KnowYourLimits --public --source . --remote origin --push
```

Если репозиторий уже создан, добавьте его как `origin` и отправьте `main`.
`OWNER` нужно заменить подтверждённым именем организации.

## Подпись и нотаризация

Установленный `zamokctl` должен поддерживать `package` и `github-release`
(подготовка проверена с версией 1.10.1). Приватный ключ Developer ID остаётся
в Keychain; публичный SHA1 сертификата можно хранить в репозитории.

Для создания профиля выполните на своём Mac:

```sh
xcrun notarytool store-credentials notarize
```

Команда интерактивно запросит Apple ID, пароль приложения и Team ID.
Не добавляйте пароль или ключ `.p8` в репозиторий.
Для другого существующего профиля:

```sh
export KYL_NOTARY_PROFILE=existing-profile
# При необходимости: export KYL_NOTARY_KEYCHAIN=/path/to/keychain-db
```

Как в Tish, поддерживается ключ App Store Connect:

```sh
export APPLE_API_KEY_PATH=/private/path/AuthKey.p8
export APPLE_API_KEY=KEY_ID
export APPLE_API_ISSUER=ISSUER_UUID
```

Три параметра API задаются вместе. При их наличии используется API-ключ.
Пароли, сертификаты и ключи не требуются для проверки исходников в CI.

## Проверки и релиз

```sh
bun install --frozen-lockfile
bun run check:release
bun run check
bun run test
bun run test:release
bun run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
bunx playwright install chromium
bun run test:e2e
```

Создание подписанного DMG без загрузки на GitHub:

```sh
bun run package:macos
```

Существующий `.app` можно упаковать с `--skip-build`, но для GitHub-релиза
всегда выполняется новая сборка. Отсутствующие credentials обнаруживаются
до затратной сборки Tauri.

Проверить план без сборки и сетевых обращений:

```sh
export KYL_GITHUB_REPOSITORY=OWNER/KnowYourLimits
bun run release:github --dry-run
```

Создать черновик релиза из чистого, уже отправленного коммита:

```sh
bun run release:github
```

Команда проверяет чистое состояние Git и LICENSE, проверяет наличие коммита на GitHub,
собирает и упаковывает `.app`, проверяет DMG и вызывает
`zamokctl github-release --no-publish` с точным SHA коммита.
`GITHUB_TOKEN` берётся из окружения либо из активного входа `gh auth login`;
значение токена не выводится.

Для непосредственной публикации вместо черновика:

```sh
bun run release:github --publish
```

Тег релиза — `v<version>`. Чтобы опубликовать уже проверенный черновик без
повторной сборки и замены подписанных байтов:

```sh
gh release edit v0.2.4 --repo OWNER/KnowYourLimits --draft=false
```

Повторный релиз с изменёнными исходниками должен получить новую версию.
GitHub Actions проверяет сборку `.app`, но скачиваемый DMG проходит отдельный
путь Developer ID и нотаризации через Zamok.

Ссылки: [Tauri — сборка в GitHub](https://v2.tauri.app/distribute/pipelines/github/),
[Tauri — подпись macOS](https://v2.tauri.app/distribute/sign/macos/).
