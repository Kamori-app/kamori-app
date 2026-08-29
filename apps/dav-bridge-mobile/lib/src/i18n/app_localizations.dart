import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

class AppLocalizations {
  const AppLocalizations(this.locale);

  final Locale locale;

  static const supportedLocales = <Locale>[Locale('en'), Locale('ru')];

  static AppLocalizations of(BuildContext context) =>
      Localizations.of<AppLocalizations>(context, AppLocalizations)!;

  static Locale resolve(Locale? deviceLocale) =>
      deviceLocale?.languageCode == 'ru'
          ? const Locale('ru')
          : const Locale('en');

  String text(String key) =>
      (_copy[locale.languageCode] ?? _copy['en']!)[key] ??
      _copy['en']![key] ??
      key;

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  static const _copy = <String, Map<String, String>>{
    'en': {
      'registration':
          'Registration is available in the Kamori web app. Use your existing account here.',
      'username': 'Username',
      'password': 'Password',
      'totp': 'TOTP (optional)',
      'login': 'Sign in',
      'passkeyPlan':
          'Mobile passkey sign-in will arrive with native platform credential support. This release uses OPAQUE password sign-in.',
      'organizer': 'Organizer',
      'invites': 'Invites',
      'logout': 'Log out',
      'encryptedOrganizer': 'Your encrypted organizer',
      'events': 'Events',
      'tasks': 'Tasks',
      'contacts': 'Contacts',
      'encryptedSync': 'Encrypted sync',
      'appliedItems': 'Applied items',
      'lastSync': 'Last sync',
      'never': 'Never',
      'syncNow': 'Sync now',
      'backgroundSync': 'Background sync',
      'backgroundSyncBody':
          'Uses native scheduled work; no localhost server is started.',
      'spaces': 'Spaces',
      'newSpace': 'New space name',
      'createSpace': 'Create space',
      'noSpaces': 'No spaces yet.',
      'syncSettings': 'Sync settings',
      'serviceUrl': 'Kamori service URL',
      'encryptedCache': 'Encrypted local cache',
      'apply': 'Apply',
      'advancedConnection': 'Advanced connection',
      'advancedConnectionBody':
          'Only change this when connecting to a trusted self-hosted Kamori server.',
      'serviceUrlHelp': 'HTTPS origin, without a path',
      'language': 'Language',
      'system': 'System',
      'english': 'English',
      'russian': 'Russian',
      'systemIntegration': 'System integration',
      'systemIntegrationBody':
          'Choose separately for each space whether to create plaintext copies for built-in phone apps. Kamori never runs a localhost DAV server on mobile.',
      'calendarProjection': 'Show events in Calendar',
      'calendarPermission': 'Requires full Calendar permission.',
      'contactsProjection': 'Show people in Contacts',
      'contactsPermission': 'Requires Contacts permission.',
      'tasksStay':
          'Tasks remain inside Kamori in this release. System copies are one-way projections; edit the encrypted source in Kamori.',
      'showData': 'Show Kamori data in',
      'projectionWarning':
          'This creates decrypted copies in the phone database. Device backups and other permitted apps may be able to read them. Kamori remains fully usable if you decline.',
      'notNow': 'Not now',
      'continue': 'Continue',
      'disableIntegration': 'Disable system integration?',
      'disableBody':
          'Future updates will stop. Keep or remove the Kamori-created copies already stored on this phone?',
      'cancel': 'Cancel',
      'keepCopies': 'Keep copies',
      'removeCopies': 'Remove copies',
      'refresh': 'Refresh local data',
      'all': 'All',
      'newItem': 'New item',
      'newOrganizerItem': 'New organizer item',
      'editItem': 'Edit item',
      'encryptedSpace': 'Encrypted space',
      'itemType': 'Item type',
      'calendarEvent': 'Calendar event',
      'task': 'Task',
      'contact': 'Contact',
      'fullName': 'Full name',
      'title': 'Title',
      'completed': 'Completed',
      'open': 'Open',
      'email': 'Email',
      'phone': 'Phone',
      'starts': 'Starts',
      'ends': 'Ends',
      'endsInclusive': 'End date (inclusive)',
      'save': 'Save',
      'notSet': 'Not set',
      'conflictCopy': 'Conflict copy',
      'delete': 'Delete',
      'emptyOrganizer':
          'Nothing here yet. Create an encrypted space, then add your first event, task, or contact.',
      'inviteIntro':
          'Only registered Kamori users can accept invite codes and join a space.',
      'noLocalSpaces':
          'No local spaces yet. You can still redeem an invite code below.',
      'generateInvite': 'Generate invite code',
      'space': 'Space',
      'codeLifetime': 'Code lifetime',
      'generateCode': 'Generate code',
      'shareCode': 'Share this code:',
      'validFor': 'Valid for',
      'minutes': 'minutes',
      'copyCode': 'Copy code',
      'codeCopied': 'Code copied.',
      'inviteGenerated': 'Invite code generated.',
      'redeemInvite': 'Redeem invite code',
      'inviteCode': 'Invite code',
      'joinSpace': 'Join space',
      'joinedSpace': 'Joined space',
      'dismiss': 'Dismiss',
      'searchContacts': 'Search contacts',
      'searchTasks': 'Search tasks',
      'showCompleted': 'Show completed tasks',
      'sortByName': 'Sort by name',
      'sortByFamily': 'Sort by family name',
      'sortByOrganization': 'Sort by organization',
      'displayName': 'Display name',
      'notes': 'Notes',
      'categories': 'Categories',
      'commaSeparated': 'Separate categories with commas',
      'due': 'Due',
      'priority': 'Priority',
      'none': 'None',
      'high': 'High',
      'medium': 'Medium',
      'low': 'Low',
      'allDay': 'All-day event',
      'location': 'Location',
      'repeat': 'Repeat',
      'daily': 'Daily',
      'weekly': 'Weekly',
      'monthly': 'Monthly',
      'yearly': 'Yearly',
      'reminder': 'Reminder for system/DAV apps',
      'atTime': 'At start or due time',
      'tenMinutesBefore': '10 minutes before',
      'oneHourBefore': '1 hour before',
      'oneDayBefore': '1 day before',
      'importedValue': 'Imported value',
      'givenName': 'Given name',
      'familyName': 'Family name',
      'organization': 'Organization',
      'jobTitle': 'Job title',
      'emailAddresses': 'Email addresses',
      'phoneNumbers': 'Phone numbers',
      'onePerLineWithLabel': 'One per line, for example “work: value”',
      'address': 'Address',
      'addAddress': 'Add address',
      'removeAddress': 'Remove address',
      'label': 'Label',
      'namePrefix': 'Name prefix',
      'middleName': 'Middle name',
      'nameSuffix': 'Name suffix',
      'street': 'Street',
      'city': 'City',
      'region': 'Region',
      'postalCode': 'Postal code',
      'country': 'Country',
      'birthday': 'Birthday',
      'website': 'Website',
      'favorite': 'Favorite contact',
      'titleRequired': 'Enter a title or display name.',
      'validEventRange': 'Choose a start and an end later than the start.',
      'saveFailed':
          'The item could not be saved. Review the error above and try again.',
      'emptyTasks':
          'No open tasks. Create one with a due time, priority, or DAV reminder metadata.',
      'emptyEvents': 'No events on this date. Tap New item to schedule one.',
      'emptyContacts':
          'No matching contacts. Add a person or change the search.',
    },
    'ru': {
      'registration':
          'Регистрация доступна в веб-приложении Kamori. Здесь войдите в существующий аккаунт.',
      'username': 'Имя пользователя',
      'password': 'Пароль',
      'totp': 'TOTP (необязательно)',
      'login': 'Войти',
      'passkeyPlan':
          'Вход с passkey появится вместе с нативной поддержкой учётных данных платформы. В этой версии используется парольный вход OPAQUE.',
      'organizer': 'Органайзер',
      'invites': 'Приглашения',
      'logout': 'Выйти',
      'encryptedOrganizer': 'Ваш зашифрованный органайзер',
      'events': 'События',
      'tasks': 'Задачи',
      'contacts': 'Контакты',
      'encryptedSync': 'Зашифрованная синхронизация',
      'appliedItems': 'Применено элементов',
      'lastSync': 'Последняя синхронизация',
      'never': 'Никогда',
      'syncNow': 'Синхронизировать',
      'backgroundSync': 'Фоновая синхронизация',
      'backgroundSyncBody':
          'Использует системные фоновые задачи; localhost-сервер не запускается.',
      'spaces': 'Пространства',
      'newSpace': 'Название пространства',
      'createSpace': 'Создать пространство',
      'noSpaces': 'Пространств пока нет.',
      'syncSettings': 'Настройки синхронизации',
      'serviceUrl': 'Адрес сервиса Kamori',
      'encryptedCache': 'Зашифрованный локальный кеш',
      'apply': 'Применить',
      'advancedConnection': 'Расширенные настройки подключения',
      'advancedConnectionBody':
          'Меняйте адрес только для подключения к доверенному self-hosted серверу Kamori.',
      'serviceUrlHelp': 'HTTPS-адрес без дополнительного пути',
      'language': 'Язык',
      'system': 'Системный',
      'english': 'Английский',
      'russian': 'Русский',
      'systemIntegration': 'Системная интеграция',
      'systemIntegrationBody':
          'Для каждого пространства отдельно выберите, создавать ли расшифрованные копии во встроенных приложениях телефона. На мобильных устройствах Kamori не запускает localhost DAV-сервер.',
      'calendarProjection': 'Показывать события в Календаре',
      'calendarPermission': 'Нужен полный доступ к Календарю.',
      'contactsProjection': 'Показывать людей в Контактах',
      'contactsPermission': 'Нужен доступ к Контактам.',
      'tasksStay':
          'В этой версии задачи остаются внутри Kamori. Системные копии односторонние; редактируйте зашифрованный оригинал в Kamori.',
      'showData': 'Показать данные Kamori в',
      'projectionWarning':
          'На телефоне появятся расшифрованные копии. Их смогут читать резервные копии устройства и другие приложения с разрешением. Если отказаться, Kamori продолжит работать полностью.',
      'notNow': 'Не сейчас',
      'continue': 'Продолжить',
      'disableIntegration': 'Отключить системную интеграцию?',
      'disableBody':
          'Дальнейшие обновления прекратятся. Сохранить или удалить уже созданные Kamori копии на этом телефоне?',
      'cancel': 'Отмена',
      'keepCopies': 'Сохранить копии',
      'removeCopies': 'Удалить копии',
      'refresh': 'Обновить локальные данные',
      'all': 'Все',
      'newItem': 'Новый элемент',
      'newOrganizerItem': 'Новый элемент органайзера',
      'editItem': 'Изменить элемент',
      'encryptedSpace': 'Зашифрованное пространство',
      'itemType': 'Тип элемента',
      'calendarEvent': 'Событие календаря',
      'task': 'Задача',
      'contact': 'Контакт',
      'fullName': 'Полное имя',
      'title': 'Название',
      'completed': 'Выполнено',
      'open': 'Открыто',
      'email': 'Email',
      'phone': 'Телефон',
      'starts': 'Начало',
      'ends': 'Окончание',
      'endsInclusive': 'Дата окончания (включительно)',
      'save': 'Сохранить',
      'notSet': 'Не задано',
      'conflictCopy': 'Конфликтующая копия',
      'delete': 'Удалить',
      'emptyOrganizer':
          'Здесь пока пусто. Создайте зашифрованное пространство, затем добавьте событие, задачу или контакт.',
      'inviteIntro':
          'Только зарегистрированные пользователи Kamori могут принять код и присоединиться к пространству.',
      'noLocalSpaces':
          'Локальных пространств пока нет. Код приглашения всё равно можно принять ниже.',
      'generateInvite': 'Создать код приглашения',
      'space': 'Пространство',
      'codeLifetime': 'Срок действия кода',
      'generateCode': 'Создать код',
      'shareCode': 'Передайте этот код:',
      'validFor': 'Действует',
      'minutes': 'минут',
      'copyCode': 'Копировать код',
      'codeCopied': 'Код скопирован.',
      'inviteGenerated': 'Код приглашения создан.',
      'redeemInvite': 'Принять приглашение',
      'inviteCode': 'Код приглашения',
      'joinSpace': 'Присоединиться',
      'joinedSpace': 'Добавлено пространство',
      'dismiss': 'Скрыть',
      'searchContacts': 'Поиск контактов',
      'searchTasks': 'Поиск задач',
      'showCompleted': 'Показывать выполненные задачи',
      'sortByName': 'Сортировать по имени',
      'sortByFamily': 'Сортировать по фамилии',
      'sortByOrganization': 'Сортировать по организации',
      'displayName': 'Отображаемое имя',
      'notes': 'Заметки',
      'categories': 'Категории',
      'commaSeparated': 'Разделяйте категории запятыми',
      'due': 'Срок',
      'priority': 'Приоритет',
      'none': 'Нет',
      'high': 'Высокий',
      'medium': 'Средний',
      'low': 'Низкий',
      'allDay': 'Событие на весь день',
      'location': 'Место',
      'repeat': 'Повтор',
      'daily': 'Ежедневно',
      'weekly': 'Еженедельно',
      'monthly': 'Ежемесячно',
      'yearly': 'Ежегодно',
      'reminder': 'Напоминание для системных/DAV-приложений',
      'atTime': 'В момент начала или срока',
      'tenMinutesBefore': 'За 10 минут',
      'oneHourBefore': 'За 1 час',
      'oneDayBefore': 'За 1 день',
      'importedValue': 'Импортированное значение',
      'givenName': 'Имя',
      'familyName': 'Фамилия',
      'organization': 'Организация',
      'jobTitle': 'Должность',
      'emailAddresses': 'Email-адреса',
      'phoneNumbers': 'Телефоны',
      'onePerLineWithLabel': 'По одному на строку, например «work: значение»',
      'address': 'Адрес',
      'addAddress': 'Добавить адрес',
      'removeAddress': 'Удалить адрес',
      'label': 'Метка',
      'namePrefix': 'Обращение',
      'middleName': 'Отчество',
      'nameSuffix': 'Суффикс имени',
      'street': 'Улица',
      'city': 'Город',
      'region': 'Регион',
      'postalCode': 'Индекс',
      'country': 'Страна',
      'birthday': 'День рождения',
      'website': 'Сайт',
      'favorite': 'Избранный контакт',
      'titleRequired': 'Введите название или отображаемое имя.',
      'validEventRange': 'Выберите начало и окончание позже начала.',
      'saveFailed':
          'Не удалось сохранить элемент. Проверьте ошибку выше и повторите.',
      'emptyTasks':
          'Открытых задач нет. Создайте задачу со сроком, приоритетом или напоминанием для DAV.',
      'emptyEvents':
          'В этот день событий нет. Нажмите «Новый элемент», чтобы запланировать событие.',
      'emptyContacts':
          'Подходящих контактов нет. Добавьте контакт или измените поиск.',
    },
  };
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  bool isSupported(Locale locale) =>
      locale.languageCode == 'en' || locale.languageCode == 'ru';

  @override
  Future<AppLocalizations> load(Locale locale) =>
      SynchronousFuture(AppLocalizations(AppLocalizations.resolve(locale)));

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

extension AppLocalizationsContext on BuildContext {
  AppLocalizations get strings => AppLocalizations.of(this);
}
