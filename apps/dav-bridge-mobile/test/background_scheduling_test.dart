import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:dav_bridge_mobile/src/services/background_sync_service.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const admission = MethodChannel('kamori/background-sync');
  const register =
      'dev.flutter.pigeon.workmanager_platform_interface.WorkmanagerHostApi.registerPeriodicTask';
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  late int submissions;
  late int checks;
  Object? answer;
  setUp(() {
    debugDefaultTargetPlatformOverride = TargetPlatform.iOS;
    submissions = 0;
    checks = 0;
    answer = true;
    messenger.setMockMessageHandler(register, (message) async {
      submissions++;
      return const StandardMessageCodec().encodeMessage([null]);
    });
    messenger.setMockMethodCallHandler(admission, (call) async {
      expect(submissions, 1);
      expect(call.method, 'isScheduled');
      checks++;
      if (answer is Exception) throw answer!;
      return answer;
    });
  });
  tearDown(() {
    debugDefaultTargetPlatformOverride = null;
    messenger.setMockMessageHandler(register, null);
    messenger.setMockMethodCallHandler(admission, null);
  });
  test('iOS returns success only after native admission', () async {
    await BackgroundSyncService.schedulePeriodicSync();
    expect(checks, 1);
  });
  test('iOS rejects a silently discarded periodic request', () async {
    answer = false;
    await expectLater(
        BackgroundSyncService.schedulePeriodicSync(), throwsStateError);
  });
  test('iOS admission channel errors are visible to the caller', () async {
    answer = PlatformException(code: 'unavailable');
    await expectLater(BackgroundSyncService.schedulePeriodicSync(),
        throwsA(isA<PlatformException>()));
  });
  test('Android scheduling does not call the iOS admission channel', () async {
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    await BackgroundSyncService.schedulePeriodicSync();
    expect(submissions, 1);
    expect(checks, 0);
  });
}
