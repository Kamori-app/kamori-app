import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:dav_bridge_mobile/src/i18n/app_localizations.dart';
import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/state/bridge_controller.dart';

class ShareScreen extends ConsumerStatefulWidget {
  const ShareScreen({super.key, this.initialCollectionId});

  final String? initialCollectionId;

  @override
  ConsumerState<ShareScreen> createState() => _ShareScreenState();
}

class _ShareScreenState extends ConsumerState<ShareScreen> {
  final _redeemCodeController = TextEditingController();
  String? _selectedCollectionId;
  int _ttlMinutes = 60;
  String? _issuedCode;

  static const List<int> _ttlOptions = <int>[
    15,
    30,
    60,
    180,
    720,
    1440,
    4320,
    10080,
  ];

  @override
  void initState() {
    super.initState();
    _selectedCollectionId = widget.initialCollectionId;
  }

  @override
  void dispose() {
    _redeemCodeController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(bridgeControllerProvider);
    final controller = ref.read(bridgeControllerProvider.notifier);
    final collections = state.collections;
    final selectedCollectionId = _selectedCollectionId ??
        (collections.isNotEmpty ? collections.first.id : null);

    final selectedCollection = _findCollection(
      collections: collections,
      collectionId: selectedCollectionId,
    );

    return Scaffold(
      appBar: AppBar(title: Text(context.strings.text('invites'))),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            Card(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Text(
                  context.strings.text('inviteIntro'),
                ),
              ),
            ),
            const SizedBox(height: 12),
            if (collections.isEmpty)
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Text(
                    context.strings.text('noLocalSpaces'),
                  ),
                ),
              )
            else
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        context.strings.text('generateInvite'),
                        style: const TextStyle(
                          fontSize: 16,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      const SizedBox(height: 8),
                      DropdownButtonFormField<String>(
                        initialValue: selectedCollection?.id,
                        items: collections
                            .map(
                              (collection) => DropdownMenuItem<String>(
                                value: collection.id,
                                child: Text(collection.name),
                              ),
                            )
                            .toList(growable: false),
                        onChanged: (value) {
                          setState(() {
                            _selectedCollectionId = value;
                          });
                        },
                        decoration: InputDecoration(
                          labelText: context.strings.text('space'),
                        ),
                      ),
                      const SizedBox(height: 10),
                      DropdownButtonFormField<int>(
                        initialValue: _ttlMinutes,
                        items: _ttlOptions
                            .map(
                              (ttl) => DropdownMenuItem<int>(
                                value: ttl,
                                child: Text(_durationLabel(context, ttl)),
                              ),
                            )
                            .toList(growable: false),
                        onChanged: (value) {
                          if (value == null) {
                            return;
                          }
                          setState(() {
                            _ttlMinutes = value;
                          });
                        },
                        decoration: InputDecoration(
                          labelText: context.strings.text('codeLifetime'),
                        ),
                      ),
                      const SizedBox(height: 12),
                      FilledButton.icon(
                        onPressed: state.isBusy || selectedCollection == null
                            ? null
                            : () => _createInviteCode(
                                  context: context,
                                  controller: controller,
                                  collectionId: selectedCollection.id,
                                ),
                        icon: const Icon(Icons.qr_code_2_outlined),
                        label: Text(context.strings.text('generateCode')),
                      ),
                      if (_issuedCode != null) ...[
                        const SizedBox(height: 12),
                        DecoratedBox(
                          decoration: BoxDecoration(
                            borderRadius: BorderRadius.circular(12),
                            color: Theme.of(context)
                                .colorScheme
                                .surfaceContainerHighest,
                          ),
                          child: Padding(
                            padding: const EdgeInsets.all(12),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  context.strings.text('shareCode'),
                                  style: const TextStyle(
                                      fontWeight: FontWeight.w600),
                                ),
                                const SizedBox(height: 6),
                                SelectableText(
                                  _issuedCode!,
                                  style: const TextStyle(
                                    fontSize: 16,
                                    fontWeight: FontWeight.w700,
                                    letterSpacing: 1,
                                  ),
                                ),
                                const SizedBox(height: 4),
                                Text(
                                  '${context.strings.text('validFor')}: ${_durationLabel(context, _ttlMinutes)}.',
                                ),
                                const SizedBox(height: 8),
                                OutlinedButton.icon(
                                  onPressed: () async {
                                    await Clipboard.setData(
                                      ClipboardData(text: _issuedCode!),
                                    );
                                    if (context.mounted) {
                                      _showSnackBar(
                                        context,
                                        context.strings.text('codeCopied'),
                                      );
                                    }
                                  },
                                  icon: const Icon(Icons.copy_outlined),
                                  label: Text(context.strings.text('copyCode')),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
            const SizedBox(height: 12),
            Card(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      context.strings.text('redeemInvite'),
                      style: const TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 8),
                    TextField(
                      controller: _redeemCodeController,
                      decoration: InputDecoration(
                        labelText: context.strings.text('inviteCode'),
                        hintText: 'ABCD-EFGH-JKLM-NPQR',
                      ),
                    ),
                    const SizedBox(height: 12),
                    FilledButton.icon(
                      onPressed: state.isBusy
                          ? null
                          : () => _redeemInviteCode(
                                context: context,
                                controller: controller,
                              ),
                      icon: const Icon(Icons.login),
                      label: Text(context.strings.text('joinSpace')),
                    ),
                  ],
                ),
              ),
            ),
            if (state.error != null && state.error!.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(top: 12),
                child: Card(
                  color: Theme.of(context).colorScheme.errorContainer,
                  child: Padding(
                    padding: const EdgeInsets.all(16),
                    child: Text(
                      state.error!,
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.onErrorContainer,
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }

  Future<void> _createInviteCode({
    required BuildContext context,
    required BridgeController controller,
    required String collectionId,
  }) async {
    final issued = await controller.createInviteCode(
      collectionId: collectionId,
      ttlMinutes: _ttlMinutes,
    );

    if (issued == null || !context.mounted) {
      return;
    }

    setState(() {
      _issuedCode = issued.code;
      _ttlMinutes = issued.ttlMinutes;
    });

    _showSnackBar(context, context.strings.text('inviteGenerated'));
  }

  Future<void> _redeemInviteCode({
    required BuildContext context,
    required BridgeController controller,
  }) async {
    final code = _redeemCodeController.text.trim();
    final redeemed = await controller.redeemInviteCode(code);

    if (redeemed != null && context.mounted) {
      _showSnackBar(
        context,
        '${context.strings.text('joinedSpace')} ${redeemed.collectionId.substring(0, 8)}.',
      );
      _redeemCodeController.clear();
    }
  }

  CollectionEntry? _findCollection({
    required List<CollectionEntry> collections,
    required String? collectionId,
  }) {
    if (collectionId == null) {
      return null;
    }
    for (final collection in collections) {
      if (collection.id == collectionId) {
        return collection;
      }
    }
    return null;
  }

  void _showSnackBar(BuildContext context, String message) {
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text(message)));
  }

  String _durationLabel(BuildContext context, int minutes) {
    if (minutes % 1440 == 0) {
      final days = minutes ~/ 1440;
      return context.strings.locale.languageCode == 'ru'
          ? '$days дн.'
          : '$days ${days == 1 ? 'day' : 'days'}';
    }
    if (minutes % 60 == 0) {
      final hours = minutes ~/ 60;
      return context.strings.locale.languageCode == 'ru'
          ? '$hours ч.'
          : '$hours ${hours == 1 ? 'hour' : 'hours'}';
    }
    return '$minutes ${context.strings.text('minutes')}';
  }
}
