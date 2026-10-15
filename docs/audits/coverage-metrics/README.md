# Метрики аудита

Baseline — коммит `81248927f1ad734c246f582486d9840da6b47637`. JSON содержит относительные пути и покрытие по файлам; реальные секреты и данные пользователей не использовались.

Rust: Rust 1.97.1, cargo-llvm-cov 0.9.1, workspace с `crypto-core-lib/frb`, отдельная PostgreSQL. Фильтр: `frb_generated|integration_tests|/tests/|build.rs`. Inline test modules могут оставаться в знаменателе; branch coverage не измерялся. Воспроизводимый вызов с отдельной тестовой БД:

```sh
cargo llvm-cov --workspace --features crypto-core-lib/frb --json --summary-only --ignore-filename-regex 'frb_generated|integration_tests|/tests/|build.rs' --output-path rust-summary.json
```

Flutter: `flutter test --coverage`; web: Bun unit helper LCOV. `LF/LH` — всего/покрыто строк, `FNF/FNH` — всего/покрыто функций, если доступны. Отчёты охватывают попавшие в инструментирование файлы, а не все файлы приложения или UI. Сравнение процентов само по себе не доказывает проверку бизнес-сценария: существенные регрессии перечислены в основном аудите и журнале исправлений.

Итоговый Rust snapshot относится к 69820cd; Flutter — к c170d80, web — к dab0ba8. После c170d80 менялся только сервер; Flutter/Android gates сохраняют точную исходную revision. Web denominator расширился с 10 до 13 файлов: добавлены cloud API, MessagePack transport и cookie CSRF; aggregate процент не сравнивается как прежняя фиксированная выборка. Subprocess/browser cases не полностью отражаются в Bun LCOV. Go baseline измерен отдельно на CI Go 1.25.11 (первичный PATH Go 1.27.1 давал другой процент).

Итоговый Go snapshot относится к `35a4b15` и тому же Go 1.25.11. Shell-скрипты проверены отдельными поведенческими сценариями: их строки не входят в Go statement coverage.

Финальный Rust aggregate включает test-only operations/lock_regressions.rs (1504/1552 lines), а также не исключённые по filename inline/other test modules. Прирост line coverage не выдаётся за чистый прирост production/branch coverage; бизнес-сценарии сверяются по отдельной матрице.
