import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

enum AppLanguagePreference { system, english, russian }

extension AppLanguagePreferenceValue on AppLanguagePreference {
  Locale? get locale => switch (this) {
        AppLanguagePreference.system => null,
        AppLanguagePreference.english => const Locale('en'),
        AppLanguagePreference.russian => const Locale('ru'),
      };
}

abstract interface class LocalePreferenceStorage {
  Future<AppLanguagePreference?> read();
  Future<void> write(AppLanguagePreference value);
}

class SecureLocalePreferenceStorage implements LocalePreferenceStorage {
  SecureLocalePreferenceStorage([FlutterSecureStorage? storage])
      : _storage = storage ?? const FlutterSecureStorage();

  static const _key = 'kamori.mobile.language';
  final FlutterSecureStorage _storage;

  @override
  Future<AppLanguagePreference?> read() async {
    final value = await _storage.read(key: _key);
    for (final preference in AppLanguagePreference.values) {
      if (preference.name == value) return preference;
    }
    return null;
  }

  @override
  Future<void> write(AppLanguagePreference value) =>
      _storage.write(key: _key, value: value.name);
}

final localePreferenceStorageProvider = Provider<LocalePreferenceStorage>(
  (ref) => SecureLocalePreferenceStorage(),
);

final localeControllerProvider =
    NotifierProvider<LocaleController, AppLanguagePreference>(
        LocaleController.new);

class LocaleController extends Notifier<AppLanguagePreference> {
  @override
  AppLanguagePreference build() {
    Future<void>.microtask(_restore);
    return AppLanguagePreference.system;
  }

  Future<void> _restore() async {
    try {
      final stored = await ref.read(localePreferenceStorageProvider).read();
      if (stored != null) state = stored;
    } on Exception {
      // Language preference is non-critical; keep the deterministic System
      // default when platform storage is temporarily unavailable.
    }
  }

  Future<void> setPreference(AppLanguagePreference preference) async {
    state = preference;
    try {
      await ref.read(localePreferenceStorageProvider).write(preference);
    } on Exception {
      // Keep the active in-memory choice. A later user selection retries the
      // platform write without making the rest of the app unavailable.
    }
  }
}
