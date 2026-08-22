import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:dav_bridge_mobile/src/state/locale_controller.dart';

class _MemoryLocaleStorage implements LocalePreferenceStorage {
  _MemoryLocaleStorage(this.value);

  AppLanguagePreference? value;

  @override
  Future<AppLanguagePreference?> read() async => value;

  @override
  Future<void> write(AppLanguagePreference value) async {
    this.value = value;
  }
}

void main() {
  test('restores and persists the explicit mobile language preference',
      () async {
    final storage = _MemoryLocaleStorage(AppLanguagePreference.russian);
    final container = ProviderContainer(
      overrides: [
        localePreferenceStorageProvider.overrideWithValue(storage),
      ],
    );
    addTearDown(container.dispose);

    expect(
      container.read(localeControllerProvider),
      AppLanguagePreference.system,
    );
    await Future<void>.delayed(Duration.zero);
    expect(
      container.read(localeControllerProvider),
      AppLanguagePreference.russian,
    );

    await container
        .read(localeControllerProvider.notifier)
        .setPreference(AppLanguagePreference.english);
    expect(storage.value, AppLanguagePreference.english);
    expect(
      container.read(localeControllerProvider),
      AppLanguagePreference.english,
    );
  });
}
