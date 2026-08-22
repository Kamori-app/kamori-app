import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:dav_bridge_mobile/src/i18n/app_localizations.dart';
import 'package:dav_bridge_mobile/src/ui/app_root.dart';

void main() {
  testWidgets('renders login screen on startup', (WidgetTester tester) async {
    await tester.pumpWidget(
      const ProviderScope(
        child: KamoriMobileApp(),
      ),
    );
    await tester.pump();

    expect(find.text('Kamori'), findsOneWidget);
    expect(find.text('Sign in'), findsOneWidget);
    expect(find.textContaining('native platform credential'), findsOneWidget);
    expect(
      find.textContaining(
        'Registration is available in the Kamori web app',
      ),
      findsOneWidget,
    );
  });

  test('locale resolution supports Russian and falls back to English', () {
    expect(
        AppLocalizations.resolve(const Locale('ru', 'RU')).languageCode, 'ru');
    expect(
        AppLocalizations.resolve(const Locale('en', 'GB')).languageCode, 'en');
    expect(
        AppLocalizations.resolve(const Locale('ka', 'GE')).languageCode, 'en');
    expect(AppLocalizations.resolve(null).languageCode, 'en');
  });
}
