# Исправления по аудиту Kamori и результаты проверок

Исходный коммит `8124892`; исправления выполнены в изолированном worktree. Все результаты ниже получены на синтетических данных. Основной [аудит](2026-10-15-kamori.md) описывает дефекты и пределы проверки.

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

Проверочная БД после исправления ещё не выпущенной `002`: `kamori_audit_fix1`. Старый тестовый ledger не переписывался. Перед финальным acceptance пересоздана только disposable БД проекта `kamori-audit-20261015`: старый контейнерный запуск содержал предыдущий checksum этой новой миграции. Production не затрагивался.

## DAV и native HTTPS

Коммиты `6470410`, `ec2ab13`; закрыты C01 и C05. DAV сохраняет stream/parent выбранной ветки, тип задачи и tombstone ancestry; новый href получает v5 только при отсутствии существующего ресурса. Полный graph preflight переиспользует validator reconciliation до постановки операции в outbox. Шесть регрессий ревью воспроизвели mutation при HTTP-отказе; теперь malformed sibling/seed не меняет outbox, operation states, projections или journal. Подпись проверена изменением каждого публичного поля отдельно.

Реальные HTTP-проверки покрывают task/event/contact, conflict copy, ETag retry, удаление и повторное создание tombstoned href. Итог: **62 unit +14 DAV passed**, fmt/strict Clippy чистые; отдельный FRB graph: **51 passed**. Независимое и повторное ревью пройдены.

Оба native reqwest manifest явно включают TLS. Общий production builder применяется в FRB auth и sync runtime; синтетический HTTPS-сервер проверяет trusted CA success, untrusted CA и неверный hostname. Android использует Mozilla public roots без отсутствующего JVM verifier initialization; desktop/iOS сохраняют platform trust. Android **не** использует private/user CA или custom network-security-config roots; root bundle обновляется с клиентом. Системный trust store не менялся.

Изолированные normal dependency gates для desktop/FRB/Android пройдены; `aarch64-linux-android` FRB `--lib` и desktop `--lib` компилируются. Android check потребовал NDK `llvm-ar/llvm-ranlib` и `aarch64-linux-android24-clang`. На этом этапе native упаковка и физические устройства оставались отдельными gates; compile/host TLS не выдаются за проверку device runtime.

## Desktop decode, recovery и сохранение сессии

Коммит `bbd35d9`; закрыты C02–C04. Все cloud-ответы используют human-readable MessagePack; создание и восстановление collection применяют единый HPKE-пакет для account recovery identity. Испорченный legacy recovery package переписывается только при наличии действительного current-epoch ключа у авторизованного устройства.

Startup читает тот же SQLCipher credential state, что runtime, повторяет pending request ID и сначала сохраняет выбранный OS anchor, затем отправляет refresh. Ответ проверяется на соответствие аккаунту и сохраняется до provisioning. Свежий password/browser signin атомарно заменяет только credential/scope/attempt rows, сохраняя PIM и курсоры. Legacy adoption требует точной привязки к доверенному endpoint/account credential; неоднозначное состояние не отправляется в сеть.

RED воспроизвёл UUID string decode failure, несовместимый HPKE schema и refresh reuse после потери ответа. GREEN: **18 desktop +65 core/FRB +14 DAV passed**, fmt/strict Clippy чистые, независимое ревью без замечаний. Проверены повторные crash, response loss, OS-write failure, другой account/endpoint, legacy migration и сохранение кэша после нового входа.

Принятое ограничение: если keychain отстал более чем на одно поколение runtime-ротации, требуется новый вход; старый токен не отправляется с новым ID и не вызывает отзыв остальных сессий. Реальный OS keychain и аппаратный power-loss не испытывались.

## PIM даты, частичные правки и повторения

Коммиты `4a6612a`, `40437ae`; закрыты U01–U03. Изменение названия задачи сохраняет original due record целиком; явная правка времени сохраняет его зону. Web разворачивает wall-clock повторения и применяет исходный typed TZID к каждому occurrence, затем сравнивает реальные instants. Это обходит подтверждённое host-dependent rezone поведение pinned RRule 2.8.1, сохраняя DST, COUNT и inclusive UTC UNTIL. Mobile UTC anchor больше не получает device offset второй раз.

RED: четыре ошибки редактора в Chromium, Tbilisi/New York recurrence, mobile UTC в non-UTC окружении. Ревью дополнительно выявило дорогой перебор истёкших UNTIL и host-dependent fixtures; обе ошибки воспроизведены и устранены. Generator имеет conservative floating bound, actual-instant UNTIL filter сохранён. Два bounded-process теста проверяют истёкший SECONDLY и очень длинный overlap без хрупких миллисекундных порогов.

GREEN: **60 web units** обычной командой в UTC, Asia/Tbilisi и America/Los_Angeles; **4 Chromium editor tests**, **40 Flutter tests**, модели отдельно в трёх timezone; check/build/analyze чистые. Независимое и scoped повторное ревью пройдены. Существующая политика elapsed duration и разрешения неоднозначных/nonexistent wall times сохранена; импортированные recurrence exceptions не выдаются за проверенные.

## Промежуточные общие gates

Rust 1.97.1 workspace + FRB с отдельной PostgreSQL: **142 server +65 core +14 DAV +18 desktop =239 passed**, без пропусков; общий strict Clippy и fmt чистые. LLVM line coverage при том же фильтре: **48,71% →55,19%**; ограничения inline test modules и branch coverage приведены в основном аудите. Admin: 4 unit/check/build; desktop UI: check/build; закреплённый Bun 1.3.14. Existing Rollup empty virtual env chunk notice не является Svelte diagnostic.

Предварительный Android release с реальным FRB собран; все три native ABI присутствуют. Итоговая упаковка после background/offline изменений выполнена ниже. iOS SDK query завершается кодом 69: лицензия Xcode не принята; системные условия от имени пользователя не принимались.

## Web refresh, offline restore и background projection

Коммиты `08a9961`, `dab0ba8`; закрыты U04–U08. Web объединяет same-tab callers и получает актуальный CSRF внутри cross-tab Web Lock. Потеря ответа сохраняет generation-specific request ID, настоящий revoked 401 очищает авторизацию. Без Web Locks refresh отказывает до сети отдельной non-401 ошибкой: локальная сессия сохраняется, после истечения access потребуется новый вход.

Сбой отзыва старой mobile-сессии сохраняет очередь и предупреждение, позволяя загрузить другую совпадающую offline-сессию. Ошибки provisioning/scheduling добавляются к предупреждениям. Реальный background runner проверяет preference/account, синхронизирует encrypted PIM, сохраняет token/request ID и затем обновляет текущие opt-ins без permission prompts; provider failure не отменяет sync.

Ревью обнаружило U08 — неатомарный поздний commit и shared global FRB runtime. Общий process-global Rust guard сериализует полный background job и foreground shared-state операции. Синхронно создаётся пустой opaque handle, затем acquire заполняет его; каждый stateful native Future удерживает собственную ссылку до завершения. Dart finally освобождает свою ссылку, normal engine teardown не отпускает работающий native Future. Нет новых зависимостей. Цена: foreground-действия могут ждать network/provider работу; более сложная альтернатива — отдельные runtime по сессиям.

GREEN после actual RED: **66 web units/check/build; 68 Flutter tests/analyze/coverage; 69 core/FRB +14 DAV**, strict Clippy/fmt. Двенадцать пауз проверяют snapshot read, credential read и перед projection против logout/switch/background disable/collection disable; отдельный тест — concurrent sync/logout на том же controller. Четыре native теста проверяют exclusion, owner drop, незахваченную lease и удержание native Future. Три Important и один Minor устранены; scoped re-review чистый.

iOS identifier/plist/Workmanager callback и pending-admission channel согласованы с pinned plugin; plist/Swift syntax checks проходят. Pending request не доказывает delivery. Полный iOS compile/link блокирован непринятой лицензией Xcode. Platform-channel CRUD-тест использует реальный Contacts adapter; Calendar CRUD и реальные providers/scheduler остаются physical gates. Сторонний OS CRUD после уничтожения engine не получает Rust opaque reference; abnormal provider atomicity или remote wipe не заявляются (SPEC §9 предусматривает interrupted projection и следующий reconcile).

Повторный общий Rust gate на `dab0ba8`: **243 passed, 0 skipped/failed**, workspace+FRB Clippy/fmt чистые. LLVM **14806/26759 lines (55,33%)**, функции 48,07%; Flutter **1049/4565 (22,98%)**. Web LCOV **1128/1991** нельзя напрямую сравнивать с **978/1234**: добавлены три ранее не инструментированные API/transport файла; common helper files — 989/1247 против 978/1234. Subprocess/browser выполнение не полностью отражается в Bun LCOV. JSON с относительными путями сохранён в coverage-metrics.

## Инфраструктура, буквальные секреты и резервные копии

Коммит 35a4b15; закрыты I01–I04, независимое task review без замечаний. Sourced PostgreSQL/backup environments используют POSIX literal assignments, Compose dotenv — отдельный encoder с буквальными dollar/quotes/backslash/control chars. Настоящие Bash и Compose env_file/--env-file parsing RED показывали interpolation и выполнение синтетического command marker; GREEN сохраняет значения без выполнения.

EXIT handlers обоих job scripts записывают один failed terminal heartbeat после nonzero этапа и сохраняют исходные коды 41–49; success пишет один ok. Пятнадцать настоящих script cases с fake external commands проверяют backup/check/copy/inventory/hash/missing/mismatch/success. Дополнительный RED показал ложный ok при empty DR/nonempty primary: NR==FNR заменён на идентификацию входного файла. Empty DR теперь failed, оба пустых — valid.

.dockerignore исключает root/nested SECRETS.md, .aws и audit scratch. Исходное попадание установлено по именам и COPY . .; закрытое содержимое не исследовалось. Реальный Docker COPY/context absence gate прошёл (e8757a0). Pinned Go 1.25.11 tests/coverage/vet/mod verify/gofmt, shell syntax и CI detector чистые. Go statement coverage 215/591 (36,38%); shell logic не входит в знаменатель.

## Итоговые интеграционные проверки

Полный browser acceptance на свежей disposable PostgreSQL и пересобранных cloud/web/admin: **9/9 passed, 1,2 минуты**. Реальные recovery/sharing/blob quota/offline PIM сценарии и четыре Chromium due editor cases. Production connections и deployments не выполнялись.

Android release на dab0ba8 с реальным FRB: **102,5 MB**, три ABI содержат настоящую libcrypto_core_lib.so; стандартный Cargokit сам задаёт NDK ranlib, ручные overrides не нужны. Flutter 3.47.0 предупреждает о будущем KGP compatibility в двух pinned plugins; сборка успешна, warning не подавлялся. iOS SDK exit 69 остаётся внешним лицензионным ограничением.

Перед дополнительной волной Rust **243**, web **66**, Flutter **68**, admin **4** passed; строгие линтеры/typechecks проходят. Промежуточные цифры выше сохранены с указанием этапа. Машинные [метрики](coverage-metrics/README.md), [матрица инвариантов](2026-10-15-business-tests.md) и [десять решений](2026-10-15-decisions.md) сохраняют результаты после удаления рабочего SDD scratch. Следующий раздел фиксирует дополнительные находки whole-branch review.

## Дополнительная волна по итоговому ревью

Коммит c170d80 реализовал U09/U10 и S08–S10. Actual native probes обнаружили replay secure T0 при durable T2/T1 и cold configure-before-import без refresh; PostgreSQL probes — snapshot/recovery, blob/ownership и multi-space cascade/append deadlocks. Эти случаи отсутствовали в прежних успешных gates.

Native restore теперь использует scoped durable RefreshCredentialStore до provisioning, согласует access/refresh/pending request ID с globals, отклоняет неподтверждённый старый токен до сети. Explicit fresh signin меняет только credentials, сохраняет contact row/cursor; account/endpoint mismatch не подменяет cache. Cold background импортирует credential до configure, а native import независимо перестраивает runner; missing credential не меняет предыдущую сессию. Шесть production-native/SQLCipher/HTTP регрессий прошли.

Snapshot rotation блокирует owner до signer device; ownership acceptance берёт space, quota731, затем отсортированные accounts с NO KEY UPDATE и повторной проверкой recipient. Account deletion блокирует полный видимый workspace/space set, затем counters; maintenance совместно блокирует purge/expired-pending ресурсы. Три SQL RED показали настоящий 40P01 через production functions; четыре GREEN теста включают оба порядка recovery/rotation и mixed-owner cleanup с renewal upload lease.

Первый вариант разделения maintenance на транзакции был дважды отклонён automatic approval review; команды не выполнились. Применён разрешённый вариант с неизменными single transaction, gate44200619, delete predicates и metadata/object-queue atomicity. Он прошёл isolated-schema test; approval blocker снят без production cleanup и без запроса разрешения у пользователя.

Native HTTP harness сам выявил intermittent WouldBlock: принятый socket сохранял nonblocking mode. Отдельный std-only probe показал immediate WouldBlock вместо read timeout; явный blocking mode устранил причину без ослабления assertions. Матрица 6 тестов прошла пять раз подряд, затем полный workspace.

На c170d80: **146 server +75 core/FRB +14 DAV +18 desktop =253 passed**, 0 failed/skipped/warnings; workspace strict Clippy/fmt чистые. Flutter **70**/analyze чистые; LCOV **1055/4577 (23,05%)**. LLVM **16464/27506 (59,86%)**, functions **1376/2656 (51,81%)**; в raw aggregate входит новый test-only lock_regressions.rs (627/636), поэтому прирост не объявляется чистым production coverage.

Повторный Android release с реальным FRB: **102,6 MB**, три ABI проверены. Полный fresh-DB/rebuilt-images browser acceptance: **9/9 passed, 1,2 минуты**. Web/Go исходники не менялись: их отдельные snapshots сохраняют прежние revisions.

Scoped re-review подтвердил F1–F4 (U09/U10/S08/S09), но оставил F5/S10 открытым: новый workspace может быть создан после prelock и попасть в последующий неограниченный cascade. Actual SQL reproduction дал deadlock; существующий two-space тест создавал оба space заранее. Работа продолжена узким исправлением admission через общий account gate913, с новыми production-регрессиями team/personal/offer. Остаточное исправление и его отдельные доказательства приведены ниже.


## Остаточный S10: полнота набора при позднем admission

Коммиты `9960315`, `69820cd`. Первый scoped re-review не принял S10: FK KEY SHARE допускает создание нового workspace после обнаружения deletion set. Новый production RED воспроизвёл deadlock через team и personal create, настоящий create_space с device package и signed append; отдельные случаи проверили workspace ownership transfer и creation-first. Личный workspace, его lookup/create и membership теперь находятся в одной транзакции с account-security gate913. Team creation берёт тот же gate до resource/user locks и после ожидания проверяет существование/eligibility владельца.

Именная проверка полноты затем доказала ещё одну цепочку той же ошибки: late invite redemption → create_offer → security-space accept_offer после deletion discovery. Actual RED дал PostgreSQL deadlock в accept_offer. Поэтому source/recipient gates913 в UUID-порядке распространяются на **оба** вида ownership acceptance до offer/resource locks; offer identity/expiry, текущий owner, активное membership и eligibility проверяются повторно. Subsequent space→quota731→ordered user locks сохраняют S09. Invite redemption, maintenance transaction boundaries и cleanup predicates не менялись. Signup с атомарным новым user/workspace не является admission существующего удаляемого аккаунта.

Final focused matrix: **10 passed**, full server: **151 passed**, 0 ignored/failed; strict server Clippy/fmt чистые. Тесты наблюдают ожидание точного advisory913, а не произвольный FK wait. Deletion-first даёт нормальный отказ creator/acceptor; acceptance-first сохраняет ожидаемый deletion blocker. Точные operation totals 282/564 и отсутствие владельца, успешно удалённого конкурентно, проверены. Схемы fixtures изолированы: глобальный test-only DDL не мешает параллельным deliberate phase gates. Никаких production hooks, timeout increases или deadlock retries не добавлено.

На final source `69820cd` повторены workspace LLVM + реальная PostgreSQL: **151 server +75 core/FRB +14 DAV +18 desktop =258 passed**; full workspace/all-targets FRB Clippy/fmt чистые. LLVM **17564/28473 (61,69%)**, functions **1423/2698 (52,74%)**; test-only lock_regressions.rs составляет **1504/1552** строк и явно учитывается при интерпретации. Flutter/Android исходники после c170d80 не менялись: их завершённые gates и точная provenance сохранены.

Независимое residual review ограничено `c170d80..69820cd`, находкой F5/S10 и регрессиями именно этого исправления; U09/U10/S08/S09 уже приняты предыдущим scoped review. Итоговое заключение: **F5/S10 ADDRESSED, spec PASS, quality PASS**, новых Critical/Important регрессий не найдено. F1–F4 остаются принятыми. Fresh disposable DB, пересборка cloud/web/admin и **9/9 Chromium passed (1,2 минуты)** на 69820cd. [Все независимые заключения](reviews/README.md) сохранены в репозитории.


Контрольный source-trace в residual review закрыл подозрение о create_space с явным чужим workspace_id: deletion блокирует workspace при любом существующем membership, включая revoked, до следующего SELECT spaces; create_space держит workspace/member SHARE до commit. Победивший creator виден последующему discovery; победивший deletion отзывает eligibility. Production late external-workspace membership writer не найден: space invites создают security_space_members, а workspace insert paths уже защищены admission или atomic signup. Это проверка именованного пути, не заявление о произвольном SQL.

Все восемь задач плана завершены, все 30 подтверждённых находок исправлены и приняты независимыми ревью. Пятнадцать решений с ценой ошибки архивированы отдельно. Основной checkout остался чистым на `8124892`; исправления сохранены коммитами в managed detached worktree. Deploy, push, merge, изменение реальных credentials и принятие лицензии Xcode не выполнялись. Собственные disposable контейнеры, volumes и два Rust build cache удалены после успешных проверок; APK, сырые локальные логи и нормализованные метрики сохранены. SDD scratch удаляется после коммита архива, managed worktree остаётся на месте.

Существовавшие в git подробные отчёты исполнителей сохранены вне удаляемого scratch: [Task 2](implementation-reports/2026-10-15-task-2.md), [Task 3](implementation-reports/2026-10-15-task-3.md), [Task 4](implementation-reports/2026-10-15-task-4.md), [Task 6](implementation-reports/2026-10-15-task-6.md). Их промежуточные результаты относятся к указанным в самих отчётах revisions; текущие gates приведены выше.
