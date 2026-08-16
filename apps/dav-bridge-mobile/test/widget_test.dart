import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

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
    expect(find.text('Login with Password'), findsOneWidget);
    expect(
        find.textContaining('planned after native platform'), findsOneWidget);
    expect(
      find.textContaining(
          'Registration is available only in the Kamori web portal'),
      findsOneWidget,
    );
  });
}
