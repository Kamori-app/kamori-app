# Решения, принятые при исправлениях

Это полный архив пятнадцати решений (`Ruling:`) из рабочего журнала аудита, в порядке принятия. Изменения соответствуют пользовательскому запросу немедленно исправить все подтверждённые ошибки. [Аудит](2026-10-15-kamori.md) и [матрица тестов](2026-10-15-business-tests.md) описывают доказательства и ограничения.

| № | Решение и основание | Цена ошибки / ограничение |
|---|---|---|
| 1 | «Исправить всё» охватывает все подтверждённые дефекты и их регрессии, включая дополнительные находки; непроверенные гипотезы и внешние release gates фиксируются явно | Дополнительный аудит может обнаружить новые ошибки; выполненная работа не доказывает отсутствие всех дефектов |
| 2 | Exact retry принятия invite определяется нормализованным hash кода и авторизованным получателем; новый wire request ID не добавляется | Если появится отдельное намерение повторного redemption, потребуется новый ID/миграция |
| 3 | Старое consumed invite без сохранённого original response не replayable: исторические role/floor нельзя безопасно угадать | При потере pre-upgrade ответа получателю может потребоваться новый invite |
| 4 | Android HTTPS использует bundled Mozilla public roots; desktop/iOS — platform trust. Текущий Flutter hosting не инициализирует требуемый JVM platform verifier | Android private/user CA не поддерживаются; CA bundle обновляется выпуском клиента |
| 5 | Durable native refresh получает endpoint scope; legacy adoption допустим только при точном current/predecessor OS anchor и matching account/vault. Explicit mismatch отклоняется | Неоднозначное происхождение требует fresh signin; ошибочная legacy association могла бы потерять сессию |
| 6 | Desktop отклоняет токен старше immediate predecessor до сети; fresh password/browser signin атомарно заменяет только credential rows, сохраняя PIM/cursors | После двух runtime rotations без OS persistence может потребоваться новый вход |
| 7 | Web RRULE разворачивается в floating wall-clock, каждый occurrence преобразуется через исходный typed TZID. Это обходит host-dependent rezone pinned RRule 2.8.1 | Ошибка адаптера могла бы исказить query bounds или UTC UNTIL; оба знака host offset, DST, COUNT/UNTIL покрыты регрессиями |
| 8 | iOS проверяет наличие ожидаемого pending BGTask через узкий native channel: pinned plugin проглатывает submission errors | Временная гонка pending query может показать ошибку и потребовать retry; admission не гарантирует delivery |
| 9 | Web без Web Locks прекращает automatic refresh до сети non-401 ошибкой, сохраняя локальную авторизацию | После истечения access token в таком браузере нужен новый вход, пока не реализована другая координация |
| 10 | Полный mobile background job и foreground runtime/session/token/opt-in действия используют общий process-global Rust/FRB guard; native Future удерживает владение lease | Logout/disable могут ждать медленную network/provider работу; альтернатива — отдельные runtime. Third-party OS call после engine teardown не получает Rust lease; atomic provider interruption не заявляется |
| 11 | Mobile restore применяет fail-closed политику durable generation: current/predecessor/pending восстанавливаются до сети, неподтверждённый старый токен отклоняется; explicit fresh signin заменяет только credentials | При нескольких несохранённых ротациях нужен новый вход; encrypted cache/PIM должны остаться целыми. Это исключает произвольный legacy fallback, подтверждённо вызывающий account-wide revoke |
| 12 | Все 15 set-asides итогового ревью рассмотрены явно ниже: внешние gates и непроверенные coverage gaps остаются границами доказательства; пять подтверждённых F1–F5 исправляются обязательно | Дополнительные device/integration проверки могут обнаружить новые дефекты; успешные локальные проверки не дают полной сертификации |
| 13 | Для S10 все production bulk-delete consumers предварительно блокируют полный затронутый набор workspace/space в общем порядке, затем owner accounting; direct-delete counters сохраняются | Новый bulk consumer должен соблюдать ту же дисциплину. Произвольный конкурентный ad-hoc SQL может требовать transaction retry; полная deadlock-free гарантия для него не вводится |
| 14 | Maintenance сохраняет одну транзакцию, прежний advisory gate, delete predicates и metadata/object-queue atomicity; общий набор purge/expired-blob ресурсов предварительно блокируется в совместимом порядке | Более широкий batch может задержать concurrent writers; полнота набора и порядок owner locks требуют регрессий. Изменение границ очистки отклонено автоматическим review, операции не выполнялись |
| 15 | Production workspace creation и ownership acceptance обоих видов используют тот же account-security advisory913 gate, который deletion берёт до формирования cascade set; personal ensure/membership атомарны с ним, owner проверяется после ожидания | Source/recipient gates берутся в UUID-порядке до offer/resource locks, включая позднее invite membership. Создание/ensure/transfer может ждать account security operations. Новый admission path должен соблюдать этот gate; существующий workspace/FK порядок также обязателен |

Для решения 10 синхронно создаётся пустой opaque handle с finalizer, затем acquire заполняет его; это исключает возврат уже захваченного guard в закрытый Dart port. Dart finally освобождает только свою ссылку; native Future сохраняет отдельную ссылку до завершения. Normal isolate finalization проверена по локальному runtime и [официальному контракту NativeFinalizer](https://api.dart.dev/dart-ffi/NativeFinalizer-class.html). Аварийное завершение процесса, реальный keychain и физический OS provider остаются отдельными gates.

## Разбор всех set-asides итогового ревью

| Пункт | Решение исполнителя и основание |
|---|---|
| D01: reset/refresh и passkey-registration interleavings | Оставлен явный пробел тестов M1: ревью проверило финальные transactional bindings и не нашло stale issuance. Это не дополнительная подтверждённая ошибка |
| D02: iOS compile/link | Внешний gate: SDK exit 69, лицензия не принята. Syntax/plist не объявляются compile proof |
| D03: физические providers/scheduler/permissions | Внешний gate: нет устройства; mock/channel и packaging не доказывают delivery/OS database behavior |
| D04: abnormal provider teardown | Сохраняется SPEC §9 interrupted projection/reconcile; native Future retention не распространяется на сторонний OS call, atomic rollback не заявляется |
| D05: реальный keychain/power loss | Не проверено физически. F1/U09 подтверждён native-кодом и исправляется, а не откладывается под этим ограничением |
| D06: desktop older-than-predecessor | Согласовано с решением 6: fail closed/fresh signin, PIM сохраняется |
| D07: Android private CA | Согласовано с решением 4: bundled public roots, private CA не поддерживается выбранной политикой |
| D08: browser без Web Locks | Согласовано с решением 9: refresh локально отклоняется с сохранением offline-сессии |
| D09: legacy consumed invite | Согласовано с решением 3: исторических response metadata нет, требуется новый invite |
| D10: imported recurrence exceptions | Lossless сохранение не доказывает rendering. EXDATE/RDATE/RECURRENCE-ID остаются явно непроверенными; подтверждённой новой ошибки/совместимости не заявляется |
| D11: DST nonexistent wall time | Без трассировки UI validation/persistence и заданной политики это unconfirmed concern; нельзя автоматически менять выбранное пользователем время |
| D12: прочие crash/cursor/epoch/corpus границы | Не исчерпываются новыми регрессиями; остаются release matrix gaps, без заявления полной interoperability |
| D13: hardware WebAuthn/admin lifecycle | Внешний/coverage gate: server binding tests не заменяют физический authenticator и все admin ceremonies |
| D14: прочие server/store/egress matrix gaps | Не подтверждённые дефекты: нет полного fault-injection набора. Конкретные соседние lock failures S08–S10 вошли в обязательные исправления |
| D15: production deploy/PITR/DR/external review | Вне локального audit execution: deployment не запрошен, реальные сервисы/секреты не используются. Shell/Compose tests не объявляются DR certification |
