import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:dav_bridge_mobile/src/state/bridge_controller.dart';
import 'package:dav_bridge_mobile/src/ui/auth_screen.dart';
import 'package:dav_bridge_mobile/src/ui/dashboard_screen.dart';

class KamoriMobileApp extends ConsumerStatefulWidget {
  const KamoriMobileApp({super.key});

  @override
  ConsumerState<KamoriMobileApp> createState() => _KamoriMobileAppState();
}

class _KamoriMobileAppState extends ConsumerState<KamoriMobileApp>
    with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    Future<void>.microtask(
      () =>
          ref.read(bridgeControllerProvider.notifier).restorePersistedSession(),
    );
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    ref.read(bridgeControllerProvider.notifier).handleLifecycleState(state);
  }

  @override
  Widget build(BuildContext context) {
    final bridgeState = ref.watch(bridgeControllerProvider);

    return MaterialApp(
      title: 'Kamori',
      theme: ThemeData(
        useMaterial3: true,
        colorSchemeSeed: const Color(0xFF0A7F73),
      ),
      home: bridgeState.isAuthenticated
          ? const DashboardScreen()
          : const AuthScreen(),
    );
  }
}
