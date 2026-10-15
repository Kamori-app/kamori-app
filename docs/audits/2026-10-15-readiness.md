# Продолжение аудита: готовность к релизу

Работа выполняется в `fix/audit-remediation`, baseline `dc13f19`. Это продолжение [аудита тридцати дефектов](2026-10-15-kamori.md), не повторное объявление его задач незавершёнными. Пользователь разрешил выполнить оставшиеся проверки через subagent-driven flow. [План](../superpowers/plans/2026-10-15-release-readiness.md).

Статус: **в работе**. Результаты отдельных исполнителей становятся окончательными только после независимого spec/quality review.

## Подтверждённые находки продолжения

| ID | Нарушение | Доказательство | Текущий статус |
|---|---|---|---|
| R01 | iOS AppDelegate передавал вычисленное Int в Workmanager API, ожидающий NSNumber? | Настоящий Swift compile RED после устранения отдельной настройки SDK; минимальный NSNumber(value:...) дал успешный unsigned device build | Исправлено, spec/quality review PASS |
| R02 | Внешнее удаление системной копии контакта ломает последующий reconciliation: pinned iOS API бросает ошибку отсутствующей записи, сервис ожидает null | Настоящий NativeSystemProjectionService на новом симуляторе: project → внешнее удаление созданного ID → _projectContacts/CrudApi.get PlatformException; отдельная unit-регрессия тоже RED | Исправлено, unit/native GREEN и spec/quality review PASS |
| R03 | DB integration tests без URL базы пропускают тело и учитываются как passed | Subprocess запустил ровно один настоящий admin test без обоих DB env vars; старый код passed+skip, новая регрессия RED | Исправлено; 5 focused GREEN, 156 server tests с БД; spec/quality review PASS |

R02 не выведен из ошибочного тестового ожидания get(id)==null: отдельно воспроизведён именно production-сервис. Первая ошибочная FRB fixture с не-UUID request ID также не считалась product-дефектом.

## Фактические результаты на текущем этапе

- Xcode27.0/SDK27 доступны после принятия лицензии пользователем. Установлен и доступен simulator runtime iOS26.2; создан отдельный симулятор с синтетическими данными.
- Базовые Flutter analyze и 70 tests прошли до исправлений.
- Unsigned arm64 device app с настоящим упакованным crypto_core_lib собран: 47,7 MB. Это не signed IPA и не доказательство запуска на физическом устройстве.
- Настоящие FRB/native background channel загрузились на симуляторе; disabled projection сохранила неопределённые Contacts/Calendar permissions. Calendar CRUD прошёл; Contacts create/read/update/delete также выполнены на синтетической записи. После R02 прошли все 6 native integration cases, 3 XCTest на production entrypoint с real-FRB define, Flutter analyze и 72 unit/widget tests. Последовательный порядок подготовки Flutter/SPM host обязателен; параллельный harness сбой не считался product-дефектом.
- Dart hooks_runner1.5.0 удаляет DEVELOPER_DIR, сохраняя PATH; при глобальном выборе CommandLineTools objective_c native-assets hook не находил iPhoneOS SDK. Воспроизводимый scoped xcrun runner сохраняет выбранный Xcode без изменения глобального xcode-select. Успешная сборка относится к этому явно указанному окружению.
- Для серверных задач создана собственная loopback PostgreSQL18.6; production не используется. PITR restore будет проверен на PostgreSQL16, соответствующем hosted provisioning/runbook.

## Очередь оставшихся проверок

1. Task1 завершён, независимое review принято.
2. Task2 завершён: отсутствие PostgreSQL теперь даёт явную ошибку; независимое review принято.
3. Task3 завершён: оба порядка reset/refresh и deferred passkey commit доказаны; независимое review принято.
4. Проверить реальные SQLite/IndexedDB crash boundaries и несколько browser tabs.
5. Добавить поведенческие desktop/admin UI gates.
6. Проверить corpus импортированных recurrence exceptions и DAV round-trip.
7. Измерить ограниченный локальный concurrent workload и ожидания mobile guard.
8. Выполнить настоящий disposable encrypted pgBackRest PITR и blob restore.

## Границы доказательства

Симулятор не доказывает физическую доставку background task, аппаратный power loss или поведение Keychain/Keystore на устройстве. Подпись/публикация, TestFlight, внешние DAV-клиенты и удалённый production disaster recovery остаются отдельными release gates. Глобальные настройки Xcode и реальные credentials не менялись; production, реальные Contacts/Calendar и закрытые файлы не читались.

Сборочные входы: static Debug.xcconfig отсутствовал в HEAD, хотя Xcode project ссылался на него, и локальный файл исключался root ignore. Исполнитель сохраняет две статические include-строки и узкое ios ignore exception. Bounded showBuildSettings без файла и с восстановленным файлом в обоих случаях вернул exit0, но только с файлом доступны FLUTTER_ROOT/FLUTTER_TARGET/DART_DEFINES/PODS_ROOT. Это configuration/packaging evidence; не заявляется несуществующий missing-config compile RED.


## Принятый Task1

Коммит `ac0c51e`; [отчёт исполнителя](readiness-reports/2026-10-15-task-1.md), [независимое spec/quality review](readiness-reports/2026-10-15-task-1-review.md). Critical/Important замечаний нет. Сохранены неблокирующие предупреждения XCTest toolchain (SWIFT_DEBUG_INFORMATION environment, test-framework deployment target, AppIntents metadata и restoration marker), а также будущая необходимость SPM adoption для Rust plugin. Успешный native gate не объявляется warning-free. Android release после изменения общего Dart adapter будет обновлён итоговым контроллерным прогоном.

[Решения продолжения и их цена](2026-10-15-readiness-decisions.md).

## Принятый Task2

Коммит `d4d9fd7`; [отчёт исполнителя](readiness-reports/2026-10-15-task-2.md), [независимое spec/quality review](readiness-reports/2026-10-15-task-2-review.md). 156 серверных тестов прошли с PostgreSQL18.6, включая 45 DB integrations; Clippy/fmt прошли. Отдельный запуск настоящего DB-теста без обоих URL намеренно завершился exit101 с actionable diagnostic; filtered unit-only workflow без базы остаётся рабочим (2/2). Новая subprocess-регрессия проверяет имя и ровно один запущенный test, исключая ложный успех от нулевого совпадения.

## Принятый Task3

Коммит `a3c2efe`; [отчёт исполнителя](readiness-reports/2026-10-15-task-3.md), [независимое spec/quality review](readiness-reports/2026-10-15-task-3-review.md). Два новых сценария прошли сразу на существующих guards: это дополнительное покрытие, без новой product-находки. Наблюдены шесть точных PostgreSQL backend/blocker ожиданий; проверены stale rejection, отзыв replacement session после reset, current-generation exact refresh retry, one-use passkey completion, настоящий ES256 login и device enrollment, неизменность чужого аккаунта. Полный server suite:158/158; Clippy/fmt прошли. Reset вызван на настоящей repository transaction boundary; HTTP admission reset не выдаётся за отдельный новый конкурентный тест.
