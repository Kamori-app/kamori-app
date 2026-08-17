import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:dav_bridge_mobile/src/services/background_sync_service.dart';
import 'package:dav_bridge_mobile/src/ui/app_root.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await BackgroundSyncService.initialize();
  runApp(const ProviderScope(child: KamoriMobileApp()));
}
