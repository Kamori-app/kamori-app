import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:dav_bridge_mobile/src/i18n/app_localizations.dart';
import 'package:dav_bridge_mobile/src/state/bridge_controller.dart';
import 'package:dav_bridge_mobile/src/state/locale_controller.dart';
import 'package:dav_bridge_mobile/src/ui/brand_mark.dart';

class AuthScreen extends ConsumerStatefulWidget {
  const AuthScreen({super.key});

  @override
  ConsumerState<AuthScreen> createState() => _AuthScreenState();
}

class _AuthScreenState extends ConsumerState<AuthScreen> {
  final _loginUsernameController = TextEditingController();
  final _loginPasswordController = TextEditingController();
  final _loginTotpController = TextEditingController();
  final _serviceUrlController = TextEditingController();

  @override
  void initState() {
    super.initState();
    _serviceUrlController.text =
        ref.read(bridgeControllerProvider).cloudBaseUrl;
  }

  @override
  void dispose() {
    _loginUsernameController.dispose();
    _loginPasswordController.dispose();
    _loginTotpController.dispose();
    _serviceUrlController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(bridgeControllerProvider);

    return Scaffold(
      appBar: AppBar(
        title: const KamoriAppBarTitle(),
        actions: const [_LanguageMenu()],
      ),
      body: SafeArea(
        child: Stack(
          children: [
            _LoginForm(
              usernameController: _loginUsernameController,
              passwordController: _loginPasswordController,
              totpController: _loginTotpController,
              serviceUrlController: _serviceUrlController,
              onPasswordLogin: _handlePasswordLogin,
              onServiceUrlApply: _handleServiceUrlApply,
            ),
            if (state.error != null && state.error!.isNotEmpty)
              Positioned(
                left: 16,
                right: 16,
                bottom: 16,
                child: Material(
                  borderRadius: BorderRadius.circular(10),
                  color: Theme.of(context).colorScheme.errorContainer,
                  child: Padding(
                    padding: const EdgeInsets.all(12),
                    child: Text(
                      state.error!,
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.onErrorContainer,
                      ),
                    ),
                  ),
                ),
              ),
            if (state.isBusy)
              const Positioned.fill(
                child: ColoredBox(
                  color: Color(0x66000000),
                  child: Center(child: CircularProgressIndicator()),
                ),
              ),
          ],
        ),
      ),
    );
  }

  Future<void> _handlePasswordLogin() async {
    await ref.read(bridgeControllerProvider.notifier).loginWithPassword(
          username: _loginUsernameController.text,
          password: _loginPasswordController.text,
          totpCode: _loginTotpController.text,
        );
  }

  Future<void> _handleServiceUrlApply() => ref
      .read(bridgeControllerProvider.notifier)
      .updateCloudBaseUrl(_serviceUrlController.text);
}

class _LoginForm extends StatelessWidget {
  const _LoginForm({
    required this.usernameController,
    required this.passwordController,
    required this.totpController,
    required this.serviceUrlController,
    required this.onPasswordLogin,
    required this.onServiceUrlApply,
  });

  final TextEditingController usernameController;
  final TextEditingController passwordController;
  final TextEditingController totpController;
  final TextEditingController serviceUrlController;
  final Future<void> Function() onPasswordLogin;
  final Future<void> Function() onServiceUrlApply;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16),
      child: ListView(
        children: [
          Text(context.strings.text('registration')),
          const SizedBox(height: 16),
          TextField(
            controller: usernameController,
            decoration: InputDecoration(
              labelText: context.strings.text('username'),
            ),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: passwordController,
            obscureText: true,
            decoration: InputDecoration(
              labelText: context.strings.text('password'),
            ),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: totpController,
            keyboardType: TextInputType.number,
            decoration: InputDecoration(
              labelText: context.strings.text('totp'),
            ),
          ),
          const SizedBox(height: 20),
          FilledButton(
            onPressed: onPasswordLogin,
            child: Text(context.strings.text('login')),
          ),
          const SizedBox(height: 12),
          Text(context.strings.text('passkeyPlan')),
          const SizedBox(height: 12),
          ExpansionTile(
            tilePadding: EdgeInsets.zero,
            title: Text(context.strings.text('advancedConnection')),
            subtitle: Text(context.strings.text('advancedConnectionBody')),
            children: [
              TextField(
                controller: serviceUrlController,
                keyboardType: TextInputType.url,
                autocorrect: false,
                enableSuggestions: false,
                decoration: InputDecoration(
                  labelText: context.strings.text('serviceUrl'),
                  helperText: context.strings.text('serviceUrlHelp'),
                ),
                onSubmitted: (_) => onServiceUrlApply(),
              ),
              const SizedBox(height: 8),
              Align(
                alignment: Alignment.centerRight,
                child: OutlinedButton(
                  onPressed: onServiceUrlApply,
                  child: Text(context.strings.text('apply')),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _LanguageMenu extends ConsumerWidget {
  const _LanguageMenu();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final preference = ref.watch(localeControllerProvider);
    return PopupMenuButton<AppLanguagePreference>(
      tooltip: context.strings.text('language'),
      initialValue: preference,
      icon: const Icon(Icons.language),
      onSelected: ref.read(localeControllerProvider.notifier).setPreference,
      itemBuilder: (context) => [
        PopupMenuItem(
          value: AppLanguagePreference.system,
          child: Text(context.strings.text('system')),
        ),
        PopupMenuItem(
          value: AppLanguagePreference.english,
          child: Text(context.strings.text('english')),
        ),
        PopupMenuItem(
          value: AppLanguagePreference.russian,
          child: Text(context.strings.text('russian')),
        ),
      ],
    );
  }
}
