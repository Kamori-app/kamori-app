import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/state/bridge_controller.dart';
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

    return Scaffold(
      appBar: AppBar(
        title: const Text('Kamori'),
        actions: [
          IconButton(
            tooltip: 'Organizer',
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute<void>(builder: (_) => const PimScreen()),
            ),
            icon: const Icon(Icons.view_agenda_outlined),
          ),
          IconButton(
            tooltip: 'Invites',
            onPressed: () => _openShareScreen(context),
            icon: const Icon(Icons.group_add_outlined),
          ),
          IconButton(
            tooltip: 'Logout',
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
      if (!await _confirmProjectionEnable('Calendar')) {
        return;
      }
      await controller.setCalendarProjectionEnabled(true);
      return;
    }
    final remove = await _chooseProjectionRemoval('calendar events');
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
      if (!await _confirmProjectionEnable('Contacts')) {
        return;
      }
      await controller.setContactsProjectionEnabled(true);
      return;
    }
    final remove = await _chooseProjectionRemoval('contacts');
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
            title: Text('Show Kamori data in $category?'),
            content: Text(
              'This creates decrypted copies in the phone\'s $category '
              'database. Device backups and other permitted apps may be able '
              'to read them. Kamori remains fully usable if you decline.',
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(dialogContext, false),
                child: const Text('Not now'),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(dialogContext, true),
                child: const Text('Continue'),
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
        title: const Text('Disable system integration?'),
        content: Text(
          'Future updates will stop. Do you want to keep or remove the '
          'Kamori-created $category already stored on this phone?',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('Keep copies'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('Remove copies'),
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
            const Text('Your encrypted organizer',
                style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600)),
            const SizedBox(height: 12),
            Wrap(
              spacing: 16,
              runSpacing: 12,
              children: [
                _PimDestination(
                  icon: Icons.calendar_month_outlined,
                  label: 'Events (${count(PimItemKind.calendarEvent)})',
                  onPressed: () => onOpen(PimItemKind.calendarEvent),
                ),
                _PimDestination(
                  icon: Icons.check_circle_outline,
                  label: 'Tasks (${count(PimItemKind.task)})',
                  onPressed: () => onOpen(PimItemKind.task),
                ),
                _PimDestination(
                  icon: Icons.contacts_outlined,
                  label: 'Contacts (${count(PimItemKind.contact)})',
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
    final lastSync = lastSyncAt?.toLocal().toIso8601String() ?? 'Never';
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('Encrypted sync',
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
            const SizedBox(height: 8),
            Text('Applied items: $syncedItemsTotal'),
            Text('Last sync: $lastSync'),
            const SizedBox(height: 12),
            FilledButton.icon(
              onPressed: onSyncNow,
              icon: const Icon(Icons.sync),
              label: const Text('Sync now'),
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: backgroundSyncEnabled,
              onChanged: onBackgroundSyncChanged,
              title: const Text('Background sync'),
              subtitle: const Text(
                  'Uses native scheduled work; no localhost server is started.'),
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
            const Text('Spaces',
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
            const SizedBox(height: 8),
            TextField(
                controller: nameController,
                decoration: const InputDecoration(labelText: 'New space name')),
            const SizedBox(height: 10),
            FilledButton(
                onPressed: onCreate, child: const Text('Create space')),
            if (collections.isEmpty)
              const Padding(
                  padding: EdgeInsets.only(top: 12),
                  child: Text('No spaces yet.'))
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
      required this.onApply});

  final TextEditingController cloudBaseUrlController;
  final String sqlitePath;
  final Future<void> Function() onApply;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('Sync settings',
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
            const SizedBox(height: 8),
            TextField(
                controller: cloudBaseUrlController,
                decoration:
                    const InputDecoration(labelText: 'Kamori service URL')),
            const SizedBox(height: 8),
            SelectableText('Encrypted local cache: $sqlitePath'),
            const SizedBox(height: 10),
            FilledButton(onPressed: onApply, child: const Text('Apply')),
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
            const Text(
              'System integration',
              style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            const Text(
              'Optional plaintext projections for built-in phone apps. '
              'Kamori never runs a localhost DAV server on mobile.',
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: calendarEnabled,
              onChanged: onCalendarChanged,
              title: const Text('Show events in Calendar'),
              subtitle: const Text('Requires full Calendar permission.'),
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: contactsEnabled,
              onChanged: onContactsChanged,
              title: const Text('Show people in Contacts'),
              subtitle: const Text('Requires Contacts permission.'),
            ),
            const Text(
              'Tasks remain inside Kamori in this release. System copies are '
              'one-way projections; edit the encrypted source in Kamori.',
              style: TextStyle(fontSize: 12),
            ),
          ],
        ),
      ),
    );
  }
}
