import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:dav_bridge_mobile/src/i18n/app_localizations.dart';
import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/state/bridge_controller.dart';
import 'package:dav_bridge_mobile/src/state/locale_controller.dart';
import 'package:dav_bridge_mobile/src/ui/brand_mark.dart';
import 'package:dav_bridge_mobile/src/ui/share_screen.dart';
import 'package:dav_bridge_mobile/src/ui/pim_screen.dart';

class DashboardScreen extends ConsumerStatefulWidget {
  const DashboardScreen({super.key});

  @override
  ConsumerState<DashboardScreen> createState() => _DashboardScreenState();
}

class _DashboardScreenState extends ConsumerState<DashboardScreen> {
  final _cloudBaseUrlController = TextEditingController();
  final _collectionNameController = TextEditingController();

  @override
  void initState() {
    super.initState();
    _cloudBaseUrlController.text =
        ref.read(bridgeControllerProvider).cloudBaseUrl;
  }

  @override
  void dispose() {
    _cloudBaseUrlController.dispose();
    _collectionNameController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(bridgeControllerProvider);
    final controller = ref.read(bridgeControllerProvider.notifier);
    final language = ref.watch(localeControllerProvider);

    return Scaffold(
      appBar: AppBar(
        title: const KamoriAppBarTitle(),
        actions: [
          IconButton(
            tooltip: context.strings.text('organizer'),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute<void>(builder: (_) => const PimScreen()),
            ),
            icon: const Icon(Icons.view_agenda_outlined),
          ),
          IconButton(
            tooltip: context.strings.text('invites'),
            onPressed: () => _openShareScreen(context),
            icon: const Icon(Icons.group_add_outlined),
          ),
          IconButton(
            tooltip: context.strings.text('logout'),
            onPressed: state.isBusy ? null : controller.logout,
            icon: const Icon(Icons.logout),
          ),
        ],
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            if (state.error case final error?) ...[
              _ErrorBanner(message: error),
              const SizedBox(height: 12),
            ],
            _PimOverviewCard(
              items: state.pimItems,
              onOpen: (kind) => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => PimScreen(initialKind: kind),
                ),
              ),
            ),
            const SizedBox(height: 12),
            _SyncCard(
              syncedItemsTotal: state.syncedItemsTotal,
              lastSyncAt: state.lastSyncAt,
              backgroundSyncEnabled: state.backgroundSyncEnabled,
              onSyncNow: state.isBusy ? null : controller.syncNow,
              onBackgroundSyncChanged: controller.setBackgroundSyncEnabled,
            ),
            const SizedBox(height: 12),
            _CollectionsCard(
              collections: state.collections,
              nameController: _collectionNameController,
              onCreate: () async {
                await controller
                    .createCollection(_collectionNameController.text);
                _collectionNameController.clear();
              },
              onShare: (collectionId) =>
                  _openShareScreen(context, initialCollectionId: collectionId),
              onDelete: controller.deleteCollection,
            ),
            const SizedBox(height: 12),
            _SettingsCard(
              cloudBaseUrlController: _cloudBaseUrlController,
              sqlitePath: state.sqlitePath,
              language: language,
              onLanguageChanged:
                  ref.read(localeControllerProvider.notifier).setPreference,
              onApply: () => controller.updateCloudBaseUrl(
                _cloudBaseUrlController.text,
              ),
            ),
            const SizedBox(height: 12),
            _SystemIntegrationCard(
              calendarEnabled: state.calendarProjectionEnabled,
              contactsEnabled: state.contactsProjectionEnabled,
              onCalendarChanged:
                  state.isBusy ? null : _changeCalendarProjection,
              onContactsChanged:
                  state.isBusy ? null : _changeContactsProjection,
            ),
          ],
        ),
      ),
    );
  }

  void _openShareScreen(BuildContext context, {String? initialCollectionId}) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => ShareScreen(initialCollectionId: initialCollectionId),
      ),
    );
  }

  Future<void> _changeCalendarProjection(bool enabled) async {
    final controller = ref.read(bridgeControllerProvider.notifier);
    if (enabled) {
      if (!await _confirmProjectionEnable(context.strings.text('events'))) {
        return;
      }
      await controller.setCalendarProjectionEnabled(true);
      return;
    }
    final remove =
        await _chooseProjectionRemoval(context.strings.text('events'));
    if (remove == null) {
      return;
    }
    await controller.setCalendarProjectionEnabled(
      false,
      removeProjectedData: remove,
    );
  }

  Future<void> _changeContactsProjection(bool enabled) async {
    final controller = ref.read(bridgeControllerProvider.notifier);
    if (enabled) {
      if (!await _confirmProjectionEnable(context.strings.text('contacts'))) {
        return;
      }
      await controller.setContactsProjectionEnabled(true);
      return;
    }
    final remove =
        await _chooseProjectionRemoval(context.strings.text('contacts'));
    if (remove == null) {
      return;
    }
    await controller.setContactsProjectionEnabled(
      false,
      removeProjectedData: remove,
    );
  }

  Future<bool> _confirmProjectionEnable(String category) async {
    return await showDialog<bool>(
          context: context,
          builder: (dialogContext) => AlertDialog(
            title: Text('${context.strings.text('showData')} $category?'),
            content: Text(context.strings.text('projectionWarning')),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(dialogContext, false),
                child: Text(context.strings.text('notNow')),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(dialogContext, true),
                child: Text(context.strings.text('continue')),
              ),
            ],
          ),
        ) ??
        false;
  }

  Future<bool?> _chooseProjectionRemoval(String category) {
    return showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(context.strings.text('disableIntegration')),
        content: Text('${context.strings.text('disableBody')} ($category)'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: Text(context.strings.text('cancel')),
          ),
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: Text(context.strings.text('keepCopies')),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: Text(context.strings.text('removeCopies')),
          ),
        ],
      ),
    );
  }
}

class _ErrorBanner extends StatelessWidget {
  const _ErrorBanner({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Card(
      color: Theme.of(context).colorScheme.errorContainer,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Text(message),
      ),
    );
  }
}

class _PimOverviewCard extends StatelessWidget {
  const _PimOverviewCard({required this.items, required this.onOpen});

  final List<PimItem> items;
  final ValueChanged<PimItemKind> onOpen;

  @override
  Widget build(BuildContext context) {
    int count(PimItemKind kind) =>
        items.where((item) => item.kind == kind).length;
    return Card(
      child: Padding(
        padding: EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(context.strings.text('encryptedOrganizer'),
                style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600)),
            const SizedBox(height: 12),
            Wrap(
              spacing: 16,
              runSpacing: 12,
              children: [
                _PimDestination(
                  icon: Icons.calendar_month_outlined,
                  label:
                      '${context.strings.text('events')} (${count(PimItemKind.calendarEvent)})',
                  onPressed: () => onOpen(PimItemKind.calendarEvent),
                ),
                _PimDestination(
                  icon: Icons.check_circle_outline,
                  label:
                      '${context.strings.text('tasks')} (${count(PimItemKind.task)})',
                  onPressed: () => onOpen(PimItemKind.task),
                ),
                _PimDestination(
                  icon: Icons.contacts_outlined,
                  label:
                      '${context.strings.text('contacts')} (${count(PimItemKind.contact)})',
                  onPressed: () => onOpen(PimItemKind.contact),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _PimDestination extends StatelessWidget {
  const _PimDestination({
    required this.icon,
    required this.label,
    required this.onPressed,
  });

  final IconData icon;
  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return ActionChip(
      avatar: Icon(icon, size: 18),
      label: Text(label),
      onPressed: onPressed,
    );
  }
}

class _SyncCard extends StatelessWidget {
  const _SyncCard({
    required this.syncedItemsTotal,
    required this.lastSyncAt,
    required this.backgroundSyncEnabled,
    required this.onSyncNow,
    required this.onBackgroundSyncChanged,
  });

  final int syncedItemsTotal;
  final DateTime? lastSyncAt;
  final bool backgroundSyncEnabled;
  final Future<void> Function()? onSyncNow;
  final Future<void> Function(bool) onBackgroundSyncChanged;

  @override
  Widget build(BuildContext context) {
    final lastSync = lastSyncAt?.toLocal().toIso8601String() ??
        context.strings.text('never');
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(context.strings.text('encryptedSync'),
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
            const SizedBox(height: 8),
            Text('${context.strings.text('appliedItems')}: $syncedItemsTotal'),
            Text('${context.strings.text('lastSync')}: $lastSync'),
            const SizedBox(height: 12),
            FilledButton.icon(
              onPressed: onSyncNow,
              icon: const Icon(Icons.sync),
              label: Text(context.strings.text('syncNow')),
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: backgroundSyncEnabled,
              onChanged: onBackgroundSyncChanged,
              title: Text(context.strings.text('backgroundSync')),
              subtitle: Text(context.strings.text('backgroundSyncBody')),
            ),
          ],
        ),
      ),
    );
  }
}

class _CollectionsCard extends StatelessWidget {
  const _CollectionsCard({
    required this.collections,
    required this.nameController,
    required this.onCreate,
    required this.onShare,
    required this.onDelete,
  });

  final List<CollectionEntry> collections;
  final TextEditingController nameController;
  final Future<void> Function() onCreate;
  final void Function(String) onShare;
  final Future<void> Function(String) onDelete;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(context.strings.text('spaces'),
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
            const SizedBox(height: 8),
            TextField(
                controller: nameController,
                decoration: InputDecoration(
                  labelText: context.strings.text('newSpace'),
                )),
            const SizedBox(height: 10),
            FilledButton(
                onPressed: onCreate,
                child: Text(context.strings.text('createSpace'))),
            if (collections.isEmpty)
              Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Text(context.strings.text('noSpaces')))
            else
              ...collections.map(
                (collection) => ListTile(
                  contentPadding: EdgeInsets.zero,
                  title: Text(collection.name),
                  subtitle: Text(collection.id),
                  trailing: Wrap(
                    children: [
                      IconButton(
                          onPressed: () => onShare(collection.id),
                          icon: const Icon(Icons.group_add_outlined)),
                      IconButton(
                          onPressed: () => onDelete(collection.id),
                          icon: const Icon(Icons.delete_outline)),
                    ],
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _SettingsCard extends StatelessWidget {
  const _SettingsCard(
      {required this.cloudBaseUrlController,
      required this.sqlitePath,
      required this.language,
      required this.onLanguageChanged,
      required this.onApply});

  final TextEditingController cloudBaseUrlController;
  final String sqlitePath;
  final AppLanguagePreference language;
  final ValueChanged<AppLanguagePreference> onLanguageChanged;
  final Future<void> Function() onApply;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(context.strings.text('syncSettings'),
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
            const SizedBox(height: 8),
            TextField(
                controller: cloudBaseUrlController,
                decoration: InputDecoration(
                    labelText: context.strings.text('serviceUrl'))),
            const SizedBox(height: 8),
            SelectableText(
                '${context.strings.text('encryptedCache')}: $sqlitePath'),
            const SizedBox(height: 10),
            DropdownButtonFormField<AppLanguagePreference>(
              initialValue: language,
              decoration: InputDecoration(
                labelText: context.strings.text('language'),
              ),
              items: [
                DropdownMenuItem(
                  value: AppLanguagePreference.system,
                  child: Text(context.strings.text('system')),
                ),
                DropdownMenuItem(
                  value: AppLanguagePreference.english,
                  child: Text(context.strings.text('english')),
                ),
                DropdownMenuItem(
                  value: AppLanguagePreference.russian,
                  child: Text(context.strings.text('russian')),
                ),
              ],
              onChanged: (value) {
                if (value != null) onLanguageChanged(value);
              },
            ),
            const SizedBox(height: 10),
            FilledButton(
              onPressed: onApply,
              child: Text(context.strings.text('apply')),
            ),
          ],
        ),
      ),
    );
  }
}

class _SystemIntegrationCard extends StatelessWidget {
  const _SystemIntegrationCard({
    required this.calendarEnabled,
    required this.contactsEnabled,
    required this.onCalendarChanged,
    required this.onContactsChanged,
  });

  final bool calendarEnabled;
  final bool contactsEnabled;
  final ValueChanged<bool>? onCalendarChanged;
  final ValueChanged<bool>? onContactsChanged;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              context.strings.text('systemIntegration'),
              style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            Text(context.strings.text('systemIntegrationBody')),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: calendarEnabled,
              onChanged: onCalendarChanged,
              title: Text(context.strings.text('calendarProjection')),
              subtitle: Text(context.strings.text('calendarPermission')),
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: contactsEnabled,
              onChanged: onContactsChanged,
              title: Text(context.strings.text('contactsProjection')),
              subtitle: Text(context.strings.text('contactsPermission')),
            ),
            Text(
              context.strings.text('tasksStay'),
              style: TextStyle(fontSize: 12),
            ),
          ],
        ),
      ),
    );
  }
}
