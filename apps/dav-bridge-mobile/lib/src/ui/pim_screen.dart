import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:dav_bridge_mobile/src/i18n/app_localizations.dart';
import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/state/bridge_controller.dart';

class PimScreen extends ConsumerStatefulWidget {
  const PimScreen({super.key, this.initialKind});

  final PimItemKind? initialKind;

  @override
  ConsumerState<PimScreen> createState() => _PimScreenState();
}

class _PimScreenState extends ConsumerState<PimScreen> {
  PimItemKind? _filter;

  @override
  void initState() {
    super.initState();
    _filter = widget.initialKind;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(bridgeControllerProvider.notifier).loadPimItems();
    });
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(bridgeControllerProvider);
    final visible = state.pimItems
        .where((item) => _filter == null || item.kind == _filter)
        .toList(growable: false);

    return Scaffold(
      appBar: AppBar(
        title: Text(context.strings.text('organizer')),
        actions: [
          IconButton(
            tooltip: context.strings.text('refresh'),
            onPressed: state.isBusy
                ? null
                : ref.read(bridgeControllerProvider.notifier).syncNow,
            icon: const Icon(Icons.sync),
          ),
        ],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
            child: SegmentedButton<PimItemKind?>(
              segments: [
                ButtonSegment(
                  value: null,
                  label: Text(context.strings.text('all')),
                ),
                ButtonSegment(
                  value: PimItemKind.calendarEvent,
                  icon: const Icon(Icons.calendar_month_outlined),
                  label: Text(context.strings.text('events')),
                ),
                ButtonSegment(
                  value: PimItemKind.task,
                  icon: const Icon(Icons.check_circle_outline),
                  label: Text(context.strings.text('tasks')),
                ),
                ButtonSegment(
                  value: PimItemKind.contact,
                  icon: const Icon(Icons.contacts_outlined),
                  label: Text(context.strings.text('contacts')),
                ),
              ],
              selected: {_filter},
              onSelectionChanged: (selection) {
                setState(() => _filter = selection.first);
              },
              showSelectedIcon: false,
            ),
          ),
          if (state.error case final error?)
            Padding(
              padding: const EdgeInsets.all(16),
              child: Text(error,
                  style: TextStyle(color: Theme.of(context).colorScheme.error)),
            ),
          Expanded(
            child: visible.isEmpty
                ? const _EmptyOrganizer()
                : RefreshIndicator(
                    onRefresh:
                        ref.read(bridgeControllerProvider.notifier).syncNow,
                    child: ListView.builder(
                      padding: const EdgeInsets.fromLTRB(12, 8, 12, 96),
                      itemCount: visible.length,
                      itemBuilder: (context, index) => _PimItemTile(
                        item: visible[index],
                        onEdit: () =>
                            _openEditor(context, existing: visible[index]),
                        onDelete: () => ref
                            .read(bridgeControllerProvider.notifier)
                            .deletePimItem(visible[index]),
                        onTaskChanged: visible[index].kind == PimItemKind.task
                            ? (value) => _toggleTask(visible[index], value)
                            : null,
                      ),
                    ),
                  ),
          ),
        ],
      ),
      floatingActionButton: state.collections.isEmpty
          ? null
          : FloatingActionButton.extended(
              onPressed: state.isBusy ? null : () => _openEditor(context),
              icon: const Icon(Icons.add),
              label: Text(context.strings.text('newItem')),
            ),
    );
  }

  Future<void> _toggleTask(PimItem item, bool completed) {
    return ref.read(bridgeControllerProvider.notifier).savePimItem(
          spaceId: item.spaceId,
          resourceId: item.resourceId,
          kind: item.kind,
          title: item.title,
          completed: completed,
        );
  }

  Future<void> _openEditor(BuildContext context, {PimItem? existing}) async {
    final state = ref.read(bridgeControllerProvider);
    if (state.collections.isEmpty) {
      return;
    }
    final title = TextEditingController(text: existing?.title);
    final email = TextEditingController(text: existing?.email);
    final phone = TextEditingController(text: existing?.phone);
    var kind = existing?.kind ?? _filter ?? PimItemKind.task;
    var spaceId = existing?.spaceId ?? state.collections.first.id;
    var completed = existing?.completed ?? false;
    DateTime? startsAt = _parseIcalUtc(existing?.startsAt);
    DateTime? endsAt = _parseIcalUtc(existing?.endsAt);

    await showDialog<void>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          title: Text(existing == null
              ? context.strings.text('newOrganizerItem')
              : context.strings.text('editItem')),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                DropdownButtonFormField<String>(
                  initialValue: spaceId,
                  decoration: InputDecoration(
                    labelText: context.strings.text('encryptedSpace'),
                  ),
                  items: state.collections
                      .map(
                        (space) => DropdownMenuItem(
                          value: space.id,
                          child: Text(space.name),
                        ),
                      )
                      .toList(growable: false),
                  onChanged: existing == null
                      ? (value) =>
                          setDialogState(() => spaceId = value ?? spaceId)
                      : null,
                ),
                const SizedBox(height: 12),
                DropdownButtonFormField<PimItemKind>(
                  initialValue: kind,
                  decoration: InputDecoration(
                    labelText: context.strings.text('itemType'),
                  ),
                  items: [
                    DropdownMenuItem(
                      value: PimItemKind.calendarEvent,
                      child: Text(context.strings.text('calendarEvent')),
                    ),
                    DropdownMenuItem(
                        value: PimItemKind.task,
                        child: Text(context.strings.text('task'))),
                    DropdownMenuItem(
                        value: PimItemKind.contact,
                        child: Text(context.strings.text('contact'))),
                  ],
                  onChanged: existing == null
                      ? (value) => setDialogState(() => kind = value ?? kind)
                      : null,
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: title,
                  autofocus: true,
                  maxLength: 500,
                  decoration: InputDecoration(
                    labelText: kind == PimItemKind.contact
                        ? context.strings.text('fullName')
                        : context.strings.text('title'),
                  ),
                ),
                if (kind == PimItemKind.task)
                  CheckboxListTile(
                    contentPadding: EdgeInsets.zero,
                    value: completed,
                    onChanged: (value) =>
                        setDialogState(() => completed = value ?? false),
                    title: Text(context.strings.text('completed')),
                  ),
                if (kind == PimItemKind.contact) ...[
                  TextField(
                    controller: email,
                    keyboardType: TextInputType.emailAddress,
                    decoration: InputDecoration(
                      labelText: context.strings.text('email'),
                    ),
                  ),
                  const SizedBox(height: 12),
                  TextField(
                    controller: phone,
                    keyboardType: TextInputType.phone,
                    decoration: InputDecoration(
                      labelText: context.strings.text('phone'),
                    ),
                  ),
                ],
                if (kind == PimItemKind.calendarEvent) ...[
                  ListTile(
                    contentPadding: EdgeInsets.zero,
                    title: Text(context.strings.text('starts')),
                    subtitle: Text(_displayDateTime(
                      startsAt,
                      notSet: context.strings.text('notSet'),
                    )),
                    trailing: const Icon(Icons.edit_calendar_outlined),
                    onTap: () async {
                      final value = await _pickDateTime(context, startsAt);
                      if (value != null) setDialogState(() => startsAt = value);
                    },
                  ),
                  ListTile(
                    contentPadding: EdgeInsets.zero,
                    title: Text(context.strings.text('ends')),
                    subtitle: Text(_displayDateTime(
                      endsAt,
                      notSet: context.strings.text('notSet'),
                    )),
                    trailing: const Icon(Icons.edit_calendar_outlined),
                    onTap: () async {
                      final value =
                          await _pickDateTime(context, endsAt ?? startsAt);
                      if (value != null) setDialogState(() => endsAt = value);
                    },
                  ),
                ],
              ],
            ),
          ),
          actions: [
            TextButton(
                onPressed: () => Navigator.pop(dialogContext),
                child: Text(context.strings.text('cancel'))),
            FilledButton(
              onPressed: () async {
                if (title.text.trim().isEmpty) return;
                await ref.read(bridgeControllerProvider.notifier).savePimItem(
                      spaceId: spaceId,
                      resourceId: existing?.resourceId,
                      kind: kind,
                      title: title.text,
                      completed: completed,
                      email:
                          email.text.trim().isEmpty ? null : email.text.trim(),
                      phone:
                          phone.text.trim().isEmpty ? null : phone.text.trim(),
                      startsAt: _formatIcalUtc(startsAt),
                      endsAt: _formatIcalUtc(endsAt),
                    );
                if (dialogContext.mounted) Navigator.pop(dialogContext);
              },
              child: Text(context.strings.text('save')),
            ),
          ],
        ),
      ),
    );
    title.dispose();
    email.dispose();
    phone.dispose();
  }
}

class _EmptyOrganizer extends StatelessWidget {
  const _EmptyOrganizer();

  @override
  Widget build(BuildContext context) => Center(
        child: Padding(
          padding: const EdgeInsets.all(32),
          child: Text(
            context.strings.text('emptyOrganizer'),
            textAlign: TextAlign.center,
          ),
        ),
      );
}

class _PimItemTile extends StatelessWidget {
  const _PimItemTile({
    required this.item,
    required this.onEdit,
    required this.onDelete,
    this.onTaskChanged,
  });

  final PimItem item;
  final VoidCallback onEdit;
  final VoidCallback onDelete;
  final ValueChanged<bool>? onTaskChanged;

  @override
  Widget build(BuildContext context) {
    final subtitle = switch (item.kind) {
      PimItemKind.contact =>
        [item.email, item.phone].whereType<String>().join(' · '),
      PimItemKind.calendarEvent => _displayDateTime(
          _parseIcalUtc(item.startsAt),
          notSet: context.strings.text('notSet')),
      PimItemKind.task => item.completed
          ? context.strings.text('completed')
          : context.strings.text('open'),
    };
    return Card(
      child: ListTile(
        leading: item.kind == PimItemKind.task
            ? Checkbox(
                value: item.completed,
                onChanged: (value) => onTaskChanged?.call(value ?? false),
              )
            : Icon(switch (item.kind) {
                PimItemKind.calendarEvent => Icons.calendar_month_outlined,
                PimItemKind.task => Icons.check_circle_outline,
                PimItemKind.contact => Icons.person_outline,
              }),
        title: Text(item.title),
        subtitle: Text(item.conflict
            ? '$subtitle · ${context.strings.text('conflictCopy')}'
            : subtitle),
        onTap: onEdit,
        trailing: IconButton(
          tooltip: context.strings.text('delete'),
          onPressed: onDelete,
          icon: const Icon(Icons.delete_outline),
        ),
      ),
    );
  }
}

Future<DateTime?> _pickDateTime(BuildContext context, DateTime? initial) async {
  final seed = initial?.toLocal() ?? DateTime.now();
  final date = await showDatePicker(
    context: context,
    firstDate: DateTime(1970),
    lastDate: DateTime(2100),
    initialDate: seed,
  );
  if (date == null || !context.mounted) return null;
  final time = await showTimePicker(
    context: context,
    initialTime: TimeOfDay.fromDateTime(seed),
  );
  if (time == null) return null;
  return DateTime(date.year, date.month, date.day, time.hour, time.minute)
      .toUtc();
}

String _formatIcalUtc(DateTime? value) {
  if (value == null) return '';
  final utc = value.toUtc();
  String two(int number) => number.toString().padLeft(2, '0');
  return '${utc.year}${two(utc.month)}${two(utc.day)}T${two(utc.hour)}${two(utc.minute)}${two(utc.second)}Z';
}

DateTime? _parseIcalUtc(String? value) {
  if (value == null || value.length != 16 || !value.endsWith('Z')) return null;
  try {
    return DateTime.utc(
      int.parse(value.substring(0, 4)),
      int.parse(value.substring(4, 6)),
      int.parse(value.substring(6, 8)),
      int.parse(value.substring(9, 11)),
      int.parse(value.substring(11, 13)),
      int.parse(value.substring(13, 15)),
    );
  } on FormatException {
    return null;
  }
}

String _displayDateTime(DateTime? value, {String notSet = 'Not set'}) {
  if (value == null) return notSet;
  final local = value.toLocal();
  String two(int number) => number.toString().padLeft(2, '0');
  return '${local.year}-${two(local.month)}-${two(local.day)} ${two(local.hour)}:${two(local.minute)}';
}
