# Валидация и верификация проекта E2EE

Дата: 2026-10-16. Ветка: `fix/e2ee-trust-boundaries`.
Исходная проверяемая редакция: `7d2f0d5255c8620a0fcfdf2de2eb758e1bb1c8c4`.
Итоговый документ: [спецификация, ревизия 2](../../superpowers/specs/2026-10-16-e2ee-trust-boundaries-design.md).

## Вывод и область применимости

Ревизия 2 устраняет выявленные блокирующие пробелы на уровне архитектурной спецификации. Она пригодна для подготовки подробного плана реализации. Это не одобрение пользователем, не формальное доказательство криптографического протокола и не подтверждение реализации гарантий в Kamori. Продуктовый код в рамках этой проверки не изменялся.

Валидация проверяла соответствие пяти исходным вопросам пользователя, модели враждебного сервера и продуктовым ограничениям. Верификация проверяла согласованность переходов и доверенных оснований, соответствие утверждений о текущем коде исходникам, наличие проверяемых критериев приёмки и свойства выбранного режима HPKE.

Работали три независимых reviewer-а: модель доверия, жизненный цикл/гонки, требования/текущий код/приёмка. Первоначальные заключения с замечаниями сохранены вместе с последующими scoped re-review; одобрение из прежнего архитектурного ревью не наследовалось. Проверялась архитектура, а не точные wire encodings или будущая реализация.

## Исправленные замечания

1. **H1 → VV-S1: обратная аутентификация bootstrap.** Commitment публичного ключа native аутентифицировал получателя для браузера, но не browser-approved root для native. Враждебный API мог создать свой HPKE Base ciphertext к настоящему получателю с согласованными attacker root/profile/checkpoints. Теперь обязателен независимо переданный 32-byte pairing secret, HPKE PSK без Base fallback, привязка полного возврата к локальному pending context, проверка pins и одноразовый срок жизни. P15 описывает подмену всего ответа.
2. **M1 → VV-S2: граница отзыва устройства.** Административный revoke не отзывает скопированный master/recovery key. Подозрение на компрометацию устройства с master обязательно ведёт в root/space remediation. Простое удаление certificate не объявляется восстановлением конфиденциальности; P07 проверяет retained-old-master противника.
3. **L1 → VV-L1: causal history после compaction.** UUID/digest и текущий snapshot не восстанавливают историческое состояние родителя офлайн-операции. Автоматический reseal требует проверяемого causal archive; первоначальный rollout не удаляет payload произвольно. Без base state операция остаётся `blocked-causal-history`, её parent не меняется. P10 охватывает оба исхода.
4. **L2 → VV-L2: полномочия нового root после account commit.** Прежнее состояние пространства заранее разрешает точный RecoveryIntent с ограниченной делегацией replacement root/device на finalize. Определены kit-only intent без потерянной старой session/device signature, freeze exceptions, intent revision, CAS конкурирующих recovery, append-only abort до commit и resume после него. P06 проверяет sole-owner/lost-device/crash путь.
5. **L3 → VV-L3: старые незавершённые proofs.** Разделены auth, identity, recovery authorization и inventory generations. Финальная мутация проверяет захваченное поколение; discoverable passkey ceremonies используют committed issuance barrier, а не wall clock или текущую generation только на finish. P04–P05 сохраняют новые ceremonies для разрешённых credentials.
6. **Traceability 1 → VV-T1: передача владельца.** Offer предыдущего проверенного owner и acceptance независимо проверенного successor упорядочены в журнале. Смена `owner_user_id` на сервере не меняет клиентскую authority. Workspace ownership отделено от security-space ownership; P09 сохраняет существующий сценарий передачи.
7. **Traceability 2: положительные сценарии.** P01–P10, P14–P15 задают успешные signup, own-device enrollment, owner-offline invite, password change, обычное и compromise recovery, reconciliation, ownership и causal materialization. При заранее закреплённой делегации member root допускает только свои устройства. Подлинный account trust export используется с явной границей свежести.
8. **Traceability 3: origin/storage/projection.** P11–P14 проверяют opt-in browser unlock, lock/logout, ожидаемую same-origin границу, ошибку secure storage, background policy, account scopes, per-collection projection/permissions/keep-remove, authenticated loopback DAV и реальные Rust/FRB пути. Simulator не подменяет физические свойства устройства.
9. **Traceability 4: совместимость продукта.** Явно сохранены web-only signup, native sign-in-only, одноразовые invite codes без raw member UUID UI, TTL 15–10080 минут, Android/iOS, отсутствие mobile DAV, MessagePack bytes и фиксированные cache paths. P01/P03/P13/P14 закрепляют соответствующую приёмку.
10. **Traceability 5 → VV-T5: обычный kit-v2 recovery.** Он сохраняет content root/identity и offline authorization credential по умолчанию, но явно подтверждает существующий сброс факторов/сессий/устройств. Полный kit содержит независимый seed; одни root words не дают v2 recovery. Старые author drafts сохраняются заблокированными. P05 отделён от замены скомпрометированного root.
11. **R1 → VV-R1: export под скомпрометированным master.** При повторном security review обнаружено, что старый master позволяет подделать account trust export с чужими peer pins и увести свежие ключи после смены root. Для compromise recovery такой export/manifest/checkpoint не устанавливает authority. Нужны независимые retained anchors/подтверждённые получатели либо повторная сверка. Иначе история остаётся unverified/salvage и repair pending без fresh key release. P16 воспроизводит условие атаки.

Дополнительно уточнены inventory generation при in-place updates, детерминированный порядок space/account/device locks, согласование неизвестного результата commit перед cancel и зависимость активации нового key release от проверяемого журнала. Это требования к будущей реализации, а не выполненные изменения серверных транзакций.

Неблокирующее замечание lifecycle review к P06 также исправлено: kit не восстанавливает несинхронизированные черновики, единственная копия которых осталась на утраченном устройстве. Успешное space remediation в этом сценарии требует независимо сохранённых anchors. Это видно в итоговом P06 и подтверждено последним scoped security reread; историческая формулировка lifecycle report сохранена.

## Прослеживаемость к вопросам пользователя

- **Подмена устройства сервером:** A/B, P01–P03/P15. Независимый root/pairing anchor, сертификат и prior-state-authorized admission заменяют доверие к directory metadata. Отдельно проверяется обратная подмена root без подмены recipient.
- **Офлайн-операции исключённого участника и гонки эпох:** C, P08/P10. Exact-byte reconciliation предшествует reseal; потерянный ACK не создаёт вторую логическую операцию. Недопустимый author/causal parent сохраняет видимый заблокированный draft, независимые пространства продолжают работу.
- **OPAQUE, пароль, master, recovery и потеря устройства:** E, P04–P07/P16. Разделены wrapper change, обычный recovery и compromise remediation; client-only export_key не подменяется server-known session_key. Определены поколения, handoff, kit seed, crash/resume и пределы восстановления.
- **Скрытие, перестановка и replay журнала:** D, отрицательные history cases и P08–P10. Проверяются продолжение закреплённого head, authenticated ordering, полнота известного диапазона и согласованность ветвей. Невидимый новый хвост и изолированная fork без независимого обмена не объявляются обнаружимыми.
- **Origin, устройство, системные хранилища:** F, P11–P14. Уточнены пределы local unlock, secure storage и plaintext projection. Compromised origin и уже сделанные сторонние копии остаются явными границами, а не обещанием криптографического удаления.

## Проверка фактов о текущем коде

Независимая [проверка прослеживаемости](2026-10-16-e2ee-traceability.md) подтвердила все пять исходных причин по исходникам: серверные recipient lists, browser/native bootstrap, порядок append/duplicate, курсоры и snapshot, неизменный master при password/recovery. Она также различает web и native handling пустого cursor page и остановку очередей, не приписывая им одинаковое поведение.

Сверены реальные ownership offer/acceptance, recovery factor reset, DAG parent/base-state materialization и ограничения из AGENTS.md/SPEC.md. Точные файлы и строки зафиксированы в reviewer reports относительно исходного commit. Старые описания реализованного v1 в SPEC/ADR/whitepaper в рамках этой проверки не заменялись будущими гарантиями.

## Исполненная диагностическая проверка

Для `hpke 0.14.0` выполнен [изолированный воспроизводимый эксперимент](../verification/2026-10-16-e2ee/README.md) на Rust 1.97.1: **6 passed, 0 failed**. [Полный вывод и команда](../verification/2026-10-16-e2ee/bootstrap-probe.log) сохранены.

Эксперимент воспроизводит возможность Base encrypt-to-recipient для произвольного sender, успешный возврат с правильным PSK и отказы при Base substitution, неправильном PSK и другом AAD. Он также обнаруживает допустимость пустого PSK bundle в библиотеке: приложение должно отдельно обеспечить ровно 32 случайных байта и непустой transcript-bound psk_id. Фиксированный PSK в тесте — синтетический fixture.

Это проверка поведения примитива и контрпримера к первоначальному проекту. Она не импортирует Kamori, не проверяет сохранение pairing secret/fragment или полный P15 и не считается RED/GREEN доказательством продуктового исправления.

Структурная проверка итоговых Markdown-файлов, локальных ссылок, P01–P16, отсутствие placeholders, формат диагностического Rust и `git diff --check` прошли. [Воспроизводимый checker и SHA256 итоговой спецификации](../verification/2026-10-16-e2ee/doc-check.log) сохранены. Полные Rust/Bun/Flutter product suites не запускались: application source, workspace dependencies и bindings не менялись.

## Независимые заключения и нормативные источники

- [Security model: начальные H1/M1, дополнительный R1 и окончательное scoped closure](2026-10-16-e2ee-security-validation.md).
- [Lifecycle: L1–L3 и scoped re-review](2026-10-16-e2ee-lifecycle-verification.md).
- [Requirements/source/acceptance: пять замечаний и scoped closure](2026-10-16-e2ee-traceability.md).
- [RFC 9180 §5.1.2](https://www.rfc-editor.org/rfc/rfc9180.html#section-5.1.2): HPKE PSK authentication и требования к секрету; [§9.7](https://www.rfc-editor.org/rfc/rfc9180.html#section-9.7): application non-goals.
- [RFC 9807 §3.3](https://www.rfc-editor.org/rfc/rfc9807.html#section-3.3): client-only export key и общий session key; [§1](https://www.rfc-editor.org/rfc/rfc9807.html#section-1): ограничения OPAQUE при полной компрометации сервера.
- [RFC 9420 §3](https://www.rfc-editor.org/rfc/rfc9420.html#section-3): концептуальная модель групп/эпох. Соответствие MLS не заявляется.

## Что требуется до заявления о готовой защите

Подготовить wire transcripts, canonical encodings/domain separation, лимиты полей и state machines; разложить изменения по исходникам/миграциям/клиентам. План должен явно задать lock graph и финальные transactional rechecks, bounded recovery staging/capability retention, порядок journal/secure-store anchors и downgrade barriers.

Затем нужны product RED/GREEN evidence для атак и положительных P01–P16, fault injection на каждой границе commit/lost ACK/crash, cross-client vectors, hostile-server integration и реальные packaged native builds. Это следующий этап, не незакрытые утверждения об уже написанном коде.

Не обещаются защита исполняющегося скомпрометированного origin, стирание ранее раскрытого plaintext, возвращение секретности старой истории после утечки долговременного ключа, глобальная свежесть без независимого источника и обнаружение изолированной fork без обмена checkpoints. Отсутствие независимых anchors при root compromise приводит к честному pending/repair, а не выдаче новых ключей на основании поддельного старого root.
