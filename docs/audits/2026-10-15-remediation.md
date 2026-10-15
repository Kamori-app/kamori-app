# Исправления по аудиту Kamori и результаты проверок

Исходный коммит `8124892`; исправления выполняются в изолированном worktree. Все результаты ниже получены на синтетических данных. Основной [аудит](2026-10-15-kamori.md) описывает дефекты и пределы проверки.

## Аутентификация и восстановление

Коммиты `7ae6c6e`, `51a94cf`; закрыты S04, S05, S06, S07, T01. Отложенные доказательства привязаны к поколению учётных данных и исходной сессии, окончательная проверка выполняется внутри транзакции защищённой записи. Другие recovery-попытки теряют силу при сбросе; точные повторы завершённой попытки сохраняются. Общая сериализация аккаунта устраняет подтверждённую инверсию блокировок. Admin-тесты используют уникальные synthetic credential IDs.

RED: шесть auth-регрессий создавали новую активную сессию вместо отказа; три регрессии ревью показывали два ошибочных HTTP 200 и настоящий PostgreSQL deadlock/HTTP 500. GREEN: **131 passed, 0 failed**; Rust 1.97.1, fmt и Clippy `-D warnings` чистые. Первое и повторное независимое ревью пройдены после устранения двух Important замечаний.

Новые проверки включают OPAQUE перед recovery/password change, device approval перед reset/revoke, TOTP continuation после смены пароля, enrollment/reauth после recovery, окончательную выдачу сессии против открытой reset-транзакции, две параллельные recovery-попытки, гонку start/reset и удаление аккаунта против отзыва устройства. Конкурентные проверки наблюдают реальный PostgreSQL lock wait через `pg_blocking_pids`.

Воспроизводимый серверный gate (нужна отдельная тестовая PostgreSQL, не production):

```sh
export PATH=/Users/yujin/.cargo/bin:$PATH
export KAMORI_DATABASE_URL=postgres://kamori:kamori-audit-local@127.0.0.1:55439/kamori_audit_fix1
export CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197
rustup run 1.97.1 cargo test -p cloud-server
rustup run 1.97.1 cargo fmt --all -- --check
rustup run 1.97.1 cargo clippy -p cloud-server --all-targets -- -D warnings
```

Миграции `202610150001`, `202610150003` должны применяться перед согласованной заменой серверных бинарников; старые незавершённые ephemeral grants/pending recovery starts потребуется начать заново. Полный браузерный WebAuthn race test не выполнен. Конкурентные refresh/passkey-registration проверки остаются отдельным пробелом, перечисленным для итогового ревью.

## Приглашения, квоты и конкурентные записи

Коммиты `e49312f`, `2422144`; закрыты S01–S03. Успешное принятие приглашения сохраняет исходную роль и cursor floors вместе с потреблением кода; точный повтор возвращает исходные ciphertext и метаданные без изменения членства/use count. Отзыв, смена epoch и удаление членства исключают replay. Старые использованные приглашения без записанных метаданных остаются неповторяемыми: потерянный до обновления ответ может потребовать нового приглашения.

Новая миграция `202610150002` освобождает charge удаляемого пространства и пересчитывает существующий дрейф, включая пустые аккаунты. Она берёт EXCLUSIVE table lock до DDL/пересчёта, закрывая найденную ревью гонку с row-locking append. Runtime append блокирует space → owner → member/device без SHARE→UPDATE upgrade.

RED воспроизвёл неидемпотентный retry, двойной учёт удалённых данных, deadlock двух разных операций и deadlock миграции с append. GREEN: **142 passed**, fmt/strict Clippy чистые; SQLx проверяет чистую установку и обновление с уже применённой `003`; миграционные тесты используют изолированные схемы/реальные ledger и locks. Browser sharing acceptance: **1 passed, 26,4 s**, проверены editor/reader и расшифровка note точного повтора. Независимое ревью и повторное ревью миграции пройдены.

Проверочная БД после исправления ещё не выпущенной `002`: `kamori_audit_fix1`. Старый тестовый ledger не переписывался. Перед финальным acceptance требуется обновить только disposable БД проекта `kamori-audit-20261015`, поскольку старый контейнерный запуск содержит предыдущий checksum этой новой миграции. Production не затрагивался.

## DAV и native HTTPS

Коммиты `6470410`, `ec2ab13`; закрыты C01 и C05. DAV сохраняет stream/parent выбранной ветки, тип задачи и tombstone ancestry; новый href получает v5 только при отсутствии существующего ресурса. Полный graph preflight переиспользует validator reconciliation до постановки операции в outbox. Шесть регрессий ревью воспроизвели mutation при HTTP-отказе; теперь malformed sibling/seed не меняет outbox, operation states, projections или journal. Подпись проверена изменением каждого публичного поля отдельно.

Реальные HTTP-проверки покрывают task/event/contact, conflict copy, ETag retry, удаление и повторное создание tombstoned href. Итог: **62 unit +14 DAV passed**, fmt/strict Clippy чистые; отдельный FRB graph: **51 passed**. Независимое и повторное ревью пройдены.

Оба native reqwest manifest явно включают TLS. Общий production builder применяется в FRB auth и sync runtime; синтетический HTTPS-сервер проверяет trusted CA success, untrusted CA и неверный hostname. Android использует Mozilla public roots без отсутствующего JVM verifier initialization; desktop/iOS сохраняют platform trust. Android **не** использует private/user CA или custom network-security-config roots; root bundle обновляется с клиентом. Системный trust store не менялся.

Изолированные normal dependency gates для desktop/FRB/Android пройдены; `aarch64-linux-android` FRB `--lib` и desktop `--lib` компилируются. Android check потребовал NDK `llvm-ar/llvm-ranlib` и `aarch64-linux-android24-clang`. Полная native упаковка и физические устройства остаются отдельными gates; compile/host TLS не выдаются за проверку device runtime.

## Desktop decode, recovery и сохранение сессии

Коммит `bbd35d9`; закрыты C02–C04. Все cloud-ответы используют human-readable MessagePack; создание и восстановление collection применяют единый HPKE-пакет для account recovery identity. Испорченный legacy recovery package переписывается только при наличии действительного current-epoch ключа у авторизованного устройства.

Startup читает тот же SQLCipher credential state, что runtime, повторяет pending request ID и сначала сохраняет выбранный OS anchor, затем отправляет refresh. Ответ проверяется на соответствие аккаунту и сохраняется до provisioning. Свежий password/browser signin атомарно заменяет только credential/scope/attempt rows, сохраняя PIM и курсоры. Legacy adoption требует точной привязки к доверенному endpoint/account credential; неоднозначное состояние не отправляется в сеть.

RED воспроизвёл UUID string decode failure, несовместимый HPKE schema и refresh reuse после потери ответа. GREEN: **18 desktop +65 core/FRB +14 DAV passed**, fmt/strict Clippy чистые, независимое ревью без замечаний. Проверены повторные crash, response loss, OS-write failure, другой account/endpoint, legacy migration и сохранение кэша после нового входа.

Принятое ограничение: если keychain отстал более чем на одно поколение runtime-ротации, требуется новый вход; старый токен не отправляется с новым ID и не вызывает отзыв остальных сессий. Реальный OS keychain и аппаратный power-loss не испытывались.
