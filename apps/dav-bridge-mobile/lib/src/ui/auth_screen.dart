import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:dav_bridge_mobile/src/state/bridge_controller.dart';

class AuthScreen extends ConsumerStatefulWidget {
  const AuthScreen({super.key});

  @override
  ConsumerState<AuthScreen> createState() => _AuthScreenState();
}

class _AuthScreenState extends ConsumerState<AuthScreen> {
  final _loginUsernameController = TextEditingController();
  final _loginPasswordController = TextEditingController();
  final _loginTotpController = TextEditingController();

  @override
  void dispose() {
    _loginUsernameController.dispose();
    _loginPasswordController.dispose();
    _loginTotpController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(bridgeControllerProvider);

    return Scaffold(
      appBar: AppBar(title: const Text('Kamori')),
      body: SafeArea(
        child: Stack(
          children: [
            _LoginForm(
              usernameController: _loginUsernameController,
              passwordController: _loginPasswordController,
              totpController: _loginTotpController,
              onPasswordLogin: _handlePasswordLogin,
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
}

class _LoginForm extends StatelessWidget {
  const _LoginForm({
    required this.usernameController,
    required this.passwordController,
    required this.totpController,
    required this.onPasswordLogin,
  });

  final TextEditingController usernameController;
  final TextEditingController passwordController;
  final TextEditingController totpController;
  final Future<void> Function() onPasswordLogin;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16),
      child: ListView(
        children: [
          const Text(
            'Registration is available only in the Kamori web portal. Use your existing account to sign in here.',
          ),
          const SizedBox(height: 16),
          TextField(
            controller: usernameController,
            decoration: const InputDecoration(labelText: 'Username'),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: passwordController,
            obscureText: true,
            decoration: const InputDecoration(labelText: 'Password'),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: totpController,
            keyboardType: TextInputType.number,
            decoration: const InputDecoration(labelText: 'TOTP (optional)'),
          ),
          const SizedBox(height: 20),
          FilledButton(
            onPressed: onPasswordLogin,
            child: const Text('Login with Password'),
          ),
          const SizedBox(height: 12),
          const Text(
            'Passkey sign-in on mobile is planned after native platform credential integration. Use OPAQUE password sign-in for this release.',
          ),
        ],
      ),
    );
  }
}
