import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:dav_bridge_mobile/src/services/background_sync_service.dart';
import 'package:workmanager/workmanager.dart';
import 'package:workmanager_android/workmanager_android.dart';
import 'package:workmanager_apple/workmanager_apple.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const admission = MethodChannel('kamori/background-sync');
  const register =
      'dev.flutter.pigeon.workmanager_platform_interface.WorkmanagerHostApi.registerPeriodicTask';
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  late WorkmanagerPlatform previousWorkmanagerPlatform;
  TargetPlatform? previousTargetPlatformOverride;
  late int submissions;
  late int checks;
  Object? answer;
  setUp(() {
    previousWorkmanagerPlatform = WorkmanagerPlatform.instance;
    previousTargetPlatformOverride = debugDefaultTargetPlatformOverride;
    // Workmanager selects by dart:io.Platform independently of Flutter.
    // Keep the real mobile adapter and mock only its native channel boundary.
    WorkmanagerApple.registerWith();
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
    WorkmanagerPlatform.instance = previousWorkmanagerPlatform;
    debugDefaultTargetPlatformOverride = previousTargetPlatformOverride;
    messenger.setMockMessageHandler(register, null);
    messenger.setMockMethodCallHandler(admission, null);
  });
  test('iOS returns success only after native admission', () async {
    await BackgroundSyncService.schedulePeriodicSync();
    expect(submissions, 1);
    expect(checks, 1);
  });
  test('iOS rejects a silently discarded periodic request', () async {
    answer = false;
    await expectLater(
        BackgroundSyncService.schedulePeriodicSync(), throwsStateError);
    expect(submissions, 1);
    expect(checks, 1);
  });
  test('iOS admission channel errors are visible to the caller', () async {
    answer = PlatformException(
        code: 'unavailable', message: 'Native admission status unavailable.');
    await expectLater(
      BackgroundSyncService.schedulePeriodicSync(),
      throwsA(isA<PlatformException>()
          .having((error) => error.code, 'code', 'unavailable')
          .having((error) => error.message, 'message',
              'Native admission status unavailable.')),
    );
    expect(submissions, 1);
    expect(checks, 1);
  });
  test('Android scheduling does not call the iOS admission channel', () async {
    WorkmanagerAndroid.registerWith();
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    await BackgroundSyncService.schedulePeriodicSync();
    expect(submissions, 1);
    expect(checks, 0);
  });
}
