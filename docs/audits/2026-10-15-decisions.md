# Решения, принятые при исправлениях

Это полный архив десяти решений (`Ruling:`) из рабочего журнала аудита, в порядке принятия. Изменения соответствуют пользовательскому запросу немедленно исправить все подтверждённые ошибки. [Аудит](2026-10-15-kamori.md) и [матрица тестов](2026-10-15-business-tests.md) описывают доказательства и ограничения.

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

Для решения 10 синхронно создаётся пустой opaque handle с finalizer, затем acquire заполняет его; это исключает возврат уже захваченного guard в закрытый Dart port. Dart finally освобождает только свою ссылку; native Future сохраняет отдельную ссылку до завершения. Normal isolate finalization проверена по локальному runtime и [официальному контракту NativeFinalizer](https://api.dart.dev/dart-ffi/NativeFinalizer-class.html). Аварийное завершение процесса, реальный keychain и физический OS provider остаются отдельными gates.
