# Исправления по аудиту Kamori и результаты проверок

Исходный коммит `8124892`; исправления выполняются в изолированном worktree. Все результаты ниже получены на синтетических данных. Основной [аудит](2026-10-15-kamori.md) описывает дефекты и пределы проверки.

## Аутентификация и восстановление

Коммиты `7ae6c6e`, `51a94cf`; закрыты S04, S05, S06, S07, T01. Отложенные доказательства привязаны к поколению учётных данных и исходной сессии, окончательная проверка выполняется внутри транзакции защищённой записи. Другие recovery-попытки теряют силу при сбросе; точные повторы завершённой попытки сохраняются. Общая сериализация аккаунта устраняет подтверждённую инверсию блокировок. Admin-тесты используют уникальные synthetic credential IDs.

RED: шесть auth-регрессий создавали новую активную сессию вместо отказа; три регрессии ревью показывали два ошибочных HTTP 200 и настоящий PostgreSQL deadlock/HTTP 500. GREEN: **131 passed, 0 failed**; Rust 1.97.1, fmt и Clippy `-D warnings` чистые. Первое и повторное независимое ревью пройдены после устранения двух Important замечаний.

Новые проверки включают OPAQUE перед recovery/password change, device approval перед reset/revoke, TOTP continuation после смены пароля, enrollment/reauth после recovery, окончательную выдачу сессии против открытой reset-транзакции, две параллельные recovery-попытки, гонку start/reset и удаление аккаунта против отзыва устройства. Конкурентные проверки наблюдают реальный PostgreSQL lock wait через `pg_blocking_pids`.

Воспроизводимый серверный gate (нужна отдельная тестовая PostgreSQL, не production):

```sh
export PATH=/Users/yujin/.cargo/bin:$PATH
export KAMORI_DATABASE_URL=postgres://kamori:kamori-audit-local@127.0.0.1:55439/kamori_audit
export CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197
rustup run 1.97.1 cargo test -p cloud-server
rustup run 1.97.1 cargo fmt --all -- --check
rustup run 1.97.1 cargo clippy -p cloud-server --all-targets -- -D warnings
```

Миграции `202610150001`, `202610150003` должны применяться перед согласованной заменой серверных бинарников; старые незавершённые ephemeral grants/pending recovery starts потребуется начать заново. Полный браузерный WebAuthn race test не выполнен. Конкурентные refresh/passkey-registration проверки остаются отдельным пробелом, перечисленным для итогового ревью.
