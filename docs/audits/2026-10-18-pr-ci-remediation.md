# Исправления проверок PR #122 и #123

Дата: 2026-10-18. Рабочая ветка: `fix/audit-remediation`. Исходное состояние: `c8fddb9`.
План: [PR CI remediation](../superpowers/plans/2026-10-18-pr-ci-remediation.md).
Текущие удалённые проверки и зависимости PR: [#122](https://github.com/Kamori-app/kamori-app/pull/122), [#123](https://github.com/Kamori-app/kamori-app/pull/123).

## Причины и исправления

**Зависимости.** Исходный [security audit](https://github.com/Kamori-app/kamori-app/actions/runs/39117486416) обнаружил девять JS advisories, семь Rust vulnerabilities и три новых запрещённых предупреждения о неподдерживаемых crates. Минимальные обновления: devalue5.9.3, source-map-js1.2.2, quinn-proto0.11.18 и rustls0.23.46.

Два независимых desktop-пути удерживали zbus5: opener и default keyring wrapper. Их замена использует keyring-core1.0.0, прежние Apple/Windows native stores, Linux dbus-secret-service-keyring-store1.0.1 с crypto-rust/vendored и open5.4.1. Полный Cargo.lock сократился с731 до698 packages. Правила `scripts/cargo-audit.sh` не менялись, новых исключений нет.

Service/account identifiers, hashing и формат сохранённых secrets сохранены. Native store устанавливается до создания Entry, ошибка инициализации не включает fallback. Отдельно проверено, что locked/unavailable ошибки не превращаются в NoEntry и не запускают создание credentials. Browser-login сохраняет launch/error/pending-state semantics; фиксированная ссылка на лицензию получила отдельный native command без произвольного URL-параметра.

**Flutter на Linux.** Три падения [исходного CI](https://github.com/Kamori-app/kamori-app/actions/runs/39117486631) происходили в background_scheduling_test.dart. Flutter target override не менял host selector Workmanager. Тесты теперь явно регистрируют реальные Apple/Android adapters и восстанавливают global state. Проверка PlatformException требует точные code/message и числа вызовов, исключая прежний ложноположительный результат от assertion внутри mock channel. Production scheduling не менялся.

**S3-фикстуры.** Старые Quay server/mc pins отказывали анонимному клиенту ещё на manifest request; DockerHub не давал доступной замены тех же tags. Четыре активные ссылки acceptance/restore заменены доступным GHCR community rebuild с неизменяемым multiarch digest. Server/init commands, credentials, bucket policy, readiness и restore logic сохранены. В новом образе находится прежний mc release; реально скопированный бинарник совпал по SHA256.

Новый publisher явно раскрыт в [acceptance README](../../tests/acceptance/README.md) и [restore README](../../tests/release-readiness/restore/README.md). Digest фиксирует используемый артефакт, но не доказывает воспроизводимость сборки из плавающих upstream build inputs. Исторический evidence-2026-10-15.json не изменён.

## Выполненная локальная проверка

- Неизменённые Bun production audit и RustSec policy прошли на итоговом lockfile. Frozen Bun install не меняет lockfile.
- Flutter3.47: analyze без замечаний, focused4/4 и full95/95. Negative controls воспроизводят wrong adapter и assertion-derived false positive.
- Реальный Rust1.97.1: desktop23/23, core/FRB80 unit +15 DAV +2 interchange; строгий desktop Clippy и workspace formatting прошли. FRB population пересекается с default workspace; общий уникальный total из этих чисел не выводится.
- Frontends: web74 unit, admin4 unit +2 mounted UI, desktop6 mounted UI; check/build всех трёх packages прошли.
- Native regression controls отвергают удаление pending-key cleanup и init failure guard. Linux Secret Service в изолированной DBus/gnome-keyring сессии находит synthetic запись со старыми service/username attributes, сохраняет её при adapter reopen, выполняет password/binary CRUD и возвращает ошибки при locked/unavailable storage.
- S3 probe проверяет anonymous pull, init exit0, точный put/get, private bucket, отказ для anonymous/wrong-password/missing-object и ненулевой exit при ошибке policy. Restore image собирается, mc COPY проверен по фактическому hash/version; пять orchestration controls прошли.
- Полный acceptance на заново собранных финальных application images: **46 passed, retries0, 1.4m**. Это9 deployed-stack сценариев и37 mounted production-component сценариев с synthetic peers, а не46 отдельных full-stack sync tests.

В локальном PATH Homebrew мог перехватывать rustc/cargo-clippy даже при `rustup run1.97.1 cargo`. Финальные native проверки выполнены с bin закреплённого toolchain первым в PATH; ранние1.99 результаты не подменяют их.

Проверенные ARM64 application image IDs совпали с реально запущенными контейнерами:

```text
cloud 95d61acb6637c3619f85db8be9b710aa8b9acf4320695d31812120e3beb1dbe3
web   6a7bb69367cfd07c143c0453a902b377048e68b63a93896334ee4e98330ed268
admin 1c170e06c2348e2d9f616395cca7c87a50cf3e5bfd6e1726c487d7ba1b4c03a5
```

Private harness overrides меняли только namespace, image tags и output paths. Scenarios, auth, readiness, retries и workers сохранены. Существующие volumes/images/Playwright artifacts не перезаписывались.

## Ревью, очистка и пределы доказательств

Каждая задача проходит отдельное spec/quality review; общее ревью охватывает весь диапазон исправлений относительно c8fddb9. Детальные отчёты и scripts/logs сохранены локально в `/private/tmp/kamori-pr-fixes-20261018/`.

Ревью выявило ошибку первоначальной очистки scratch probes: новый образ создавал anonymous /data volumes, а container rm без -v оставлял их. Шесть конкретных volumes, включая первый неудачный probe, установлены по сохранённым labeled container Mounts, проверены как неиспользуемые и удалены по точным ID. Cleanup исправлен; focused create/remove/absence controls проверяют оба варианта. Четыре исходных volumes и существующие images сохранены. Полный acceptance ради этой операционной корректировки не повторялся.

В логах intentional negative controls есть ожидаемые ошибки headless prompter/GTK и удалённого cleanup guard; локальный Node также сообщил конфликт NO_COLOR/FORCE_COLOR. Они сохранены как диагностические результаты, не объявлены ошибками финальной strict Rust lint или поводом менять application behavior.

Локальные результаты относятся к macOS/ARM64 и отдельной Linux storage probe. Полная Linux Tauri compilation, Android/iOS packaging и actual amd64 acceptance проверяются GitHub CI после публикации; текущий результат фиксируется в PR. Windows runtime, физические устройства, реальные пользовательские keychains, production deployment и новое полное PITR/RPO/RTO этим изменением не сертифицируются. Описанный в PR#123 новый E2EE-протокол остаётся архитектурным проектом.
