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
  bool _showCompleted = false;
  String _query = '';
  String _contactSort = 'name';
  DateTime _calendarDay = DateTime.now();

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
    var visible = state.pimItems
        .where((item) => _filter == null || item.kind == _filter)
        .where((item) => _query.isEmpty || _searchText(item).contains(_query))
        .where((item) =>
            item.kind != PimItemKind.task || _showCompleted || !item.completed)
        .toList();
    if (_filter == PimItemKind.calendarEvent) {
      visible = visible
          .where((item) => pimIntervalOccursOnDate(item, _calendarDay))
          .toList();
    }
    visible.sort(_compareItems);

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
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.fromLTRB(12, 12, 12, 4),
            child: SegmentedButton<PimItemKind?>(
              segments: [
                ButtonSegment(
                    value: null, label: Text(context.strings.text('all'))),
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
              onSelectionChanged: (selection) =>
                  setState(() => _filter = selection.first),
              showSelectedIcon: false,
            ),
          ),
          if (_filter == PimItemKind.contact || _filter == PimItemKind.task)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
              child: SearchBar(
                hintText: context.strings.text(
                  _filter == PimItemKind.contact
                      ? 'searchContacts'
                      : 'searchTasks',
                ),
                leading: const Icon(Icons.search),
                onChanged: (value) =>
                    setState(() => _query = value.trim().toLowerCase()),
              ),
            ),
          if (_filter == PimItemKind.task)
            SwitchListTile.adaptive(
              dense: true,
              title: Text(context.strings.text('showCompleted')),
              value: _showCompleted,
              onChanged: (value) => setState(() => _showCompleted = value),
            ),
          if (_filter == PimItemKind.contact)
            Align(
              alignment: Alignment.centerRight,
              child: Padding(
                padding: const EdgeInsets.only(right: 16),
                child: DropdownButton<String>(
                  value: _contactSort,
                  items: [
                    DropdownMenuItem(
                      value: 'name',
                      child: Text(context.strings.text('sortByName')),
                    ),
                    DropdownMenuItem(
                      value: 'family',
                      child: Text(context.strings.text('sortByFamily')),
                    ),
                    DropdownMenuItem(
                      value: 'organization',
                      child: Text(context.strings.text('sortByOrganization')),
                    ),
                  ],
                  onChanged: (value) =>
                      setState(() => _contactSort = value ?? 'name'),
                ),
              ),
            ),
          if (_filter == PimItemKind.calendarEvent)
            CalendarDatePicker(
              initialDate: _calendarDay,
              firstDate: DateTime(1970),
              lastDate: DateTime(2100),
              onDateChanged: (value) => setState(() => _calendarDay = value),
            ),
          if (state.error case final error?)
            MaterialBanner(
              content: Text(error),
              actions: [
                TextButton(
                  onPressed:
                      ref.read(bridgeControllerProvider.notifier).clearError,
                  child: Text(context.strings.text('dismiss')),
                ),
              ],
            ),
          Expanded(
            child: visible.isEmpty
                ? _EmptyOrganizer(kind: _filter)
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
                            ? (value) => ref
                                .read(bridgeControllerProvider.notifier)
                                .savePimItem(PimDraft.fromItem(
                                  visible[index],
                                  completed: value,
                                ))
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

  String _searchText(PimItem item) => [
        item.title,
        item.organization,
        item.jobTitle,
        item.notes,
        ...item.emails.map((value) => value.value),
        ...item.phones.map((value) => value.value),
      ].whereType<String>().join(' ').toLowerCase();

  int _compareItems(PimItem left, PimItem right) {
    if (left.kind == PimItemKind.task && right.kind == PimItemKind.task) {
      if (left.completed != right.completed) return left.completed ? 1 : -1;
      final leftDue = left.dueAt?.toDateTime();
      final rightDue = right.dueAt?.toDateTime();
      if (leftDue != null && rightDue != null) {
        final dueOrder = leftDue.compareTo(rightDue);
        if (dueOrder != 0) return dueOrder;
      }
      if (leftDue != null) return -1;
      if (rightDue != null) return 1;
      final leftPriority = left.priority == 0 ? 10 : left.priority;
      final rightPriority = right.priority == 0 ? 10 : right.priority;
      if (leftPriority != rightPriority) {
        return leftPriority.compareTo(rightPriority);
      }
    }
    if (left.kind == PimItemKind.contact && right.kind == PimItemKind.contact) {
      final leftValue = switch (_contactSort) {
        'family' => left.familyName.isEmpty ? left.title : left.familyName,
        'organization' => left.organization?.isNotEmpty == true
            ? left.organization!
            : left.title,
        _ => left.title,
      };
      final rightValue = switch (_contactSort) {
        'family' => right.familyName.isEmpty ? right.title : right.familyName,
        'organization' => right.organization?.isNotEmpty == true
            ? right.organization!
            : right.title,
        _ => right.title,
      };
      return leftValue.toLowerCase().compareTo(rightValue.toLowerCase());
    }
    return left.title.toLowerCase().compareTo(right.title.toLowerCase());
  }

  Future<void> _openEditor(BuildContext context, {PimItem? existing}) async {
    final state = ref.read(bridgeControllerProvider);
    if (state.collections.isEmpty) return;
    final initial = existing == null
        ? PimDraft(
            spaceId: state.collections.first.id,
            kind: _filter ?? PimItemKind.task,
            title: '',
          )
        : PimDraft.fromItem(existing);
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      builder: (sheetContext) => _PimEditorSheet(
        initial: initial,
        spaces: state.collections,
        kindLocked: existing != null,
        onSave: (draft) =>
            ref.read(bridgeControllerProvider.notifier).savePimItem(draft),
      ),
    );
  }
}

class _PimEditorSheet extends StatefulWidget {
  const _PimEditorSheet({
    required this.initial,
    required this.spaces,
    required this.kindLocked,
    required this.onSave,
  });

  final PimDraft initial;
  final List<CollectionEntry> spaces;
  final bool kindLocked;
  final Future<bool> Function(PimDraft) onSave;

  @override
  State<_PimEditorSheet> createState() => _PimEditorSheetState();
}

class _AddressControllers {
  _AddressControllers({PimPostalAddress? value}) {
    rawHead = value?.rawHead;
    poBox = value?.poBox ?? '';
    extended = value?.extended ?? '';
    label.text = value?.label ?? 'home';
    street.text = value?.street ?? '';
    locality.text = value?.locality ?? '';
    region.text = value?.region ?? '';
    postalCode.text = value?.postalCode ?? '';
    country.text = value?.country ?? '';
  }

  late final String? rawHead;
  late final String poBox;
  late final String extended;
  final label = TextEditingController();
  final street = TextEditingController();
  final locality = TextEditingController();
  final region = TextEditingController();
  final postalCode = TextEditingController();
  final country = TextEditingController();

  PimPostalAddress toValue() => PimPostalAddress(
        label: label.text.trim(),
        rawHead: rawHead,
        poBox: poBox,
        extended: extended,
        street: street.text.trim(),
        locality: locality.text.trim(),
        region: region.text.trim(),
        postalCode: postalCode.text.trim(),
        country: country.text.trim(),
      );

  bool get isEmpty => [street, locality, region, postalCode, country]
      .every((controller) => controller.text.trim().isEmpty);

  void dispose() {
    for (final controller in [
      label,
      street,
      locality,
      region,
      postalCode,
      country,
    ]) {
      controller.dispose();
    }
  }
}

class _PimEditorSheetState extends State<_PimEditorSheet> {
  late String _spaceId;
  late PimItemKind _kind;
  late bool _completed;
  late bool _allDay;
  late bool _favorite;
  late int _priority;
  late String _recurrence;
  late String _reminder;
  late DateTime? _startsAt;
  late DateTime? _endsAt;
  late DateTime? _dueAt;
  late bool _endWasMissing;
  bool _endTouched = false;
  final _title = TextEditingController();
  final _notes = TextEditingController();
  final _location = TextEditingController();
  final _categories = TextEditingController();
  final _prefix = TextEditingController();
  final _given = TextEditingController();
  final _middle = TextEditingController();
  final _family = TextEditingController();
  final _suffix = TextEditingController();
  final _organization = TextEditingController();
  final _jobTitle = TextEditingController();
  final _birthday = TextEditingController();
  final _url = TextEditingController();
  final _emails = TextEditingController();
  final _phones = TextEditingController();
  late List<_AddressControllers> _addresses;
  String? _error;
  bool _saving = false;

  @override
  void initState() {
    super.initState();
    final value = widget.initial;
    _spaceId = value.spaceId;
    _kind = value.kind;
    _completed = value.completed;
    _allDay = value.startsAt?.isAllDay ?? false;
    _favorite = value.favorite;
    _priority = value.priority;
    _recurrence = value.recurrenceRule ?? '';
    _reminder = value.reminderMinutes?.toString() ?? '';
    _startsAt = value.startsAt?.toEditorDateTime();
    _endsAt = value.endsAt?.toEditorDateTime();
    _endWasMissing = value.endsAt == null;
    if (_endWasMissing && _startsAt != null) {
      _endsAt = _allDay ? _startsAt : null;
    }
    if (_allDay && _endsAt != null) {
      _endsAt = _endsAt!.subtract(const Duration(days: 1));
    }
    _dueAt = value.dueAt?.toEditorDateTime();
    _title.text = value.title;
    _notes.text = value.notes ?? '';
    _location.text = value.location ?? '';
    _categories.text = value.categories.join(', ');
    _prefix.text = value.namePrefix;
    _given.text = value.givenName;
    _middle.text = value.middleName;
    _family.text = value.familyName;
    _suffix.text = value.nameSuffix;
    _organization.text = value.organization ?? '';
    _jobTitle.text = value.jobTitle ?? '';
    _birthday.text = value.birthday ?? '';
    _url.text = value.url ?? '';
    _emails.text = _formatLabeled(value.emails);
    _phones.text = _formatLabeled(value.phones);
    _addresses = value.addresses
        .map((address) => _AddressControllers(value: address))
        .toList();
  }

  @override
  void dispose() {
    for (final controller in [
      _title,
      _notes,
      _location,
      _categories,
      _prefix,
      _given,
      _middle,
      _family,
      _suffix,
      _organization,
      _jobTitle,
      _birthday,
      _url,
      _emails,
      _phones,
    ]) {
      controller.dispose();
    }
    for (final address in _addresses) {
      address.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.only(
          left: 20,
          right: 20,
          top: 16,
          bottom: MediaQuery.viewInsetsOf(context).bottom + 24,
        ),
        child: ListView(
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    widget.kindLocked
                        ? context.strings.text('editItem')
                        : context.strings.text('newOrganizerItem'),
                    style: Theme.of(context).textTheme.headlineSmall,
                  ),
                ),
                IconButton(
                  onPressed: () => Navigator.pop(context),
                  icon: const Icon(Icons.close),
                ),
              ],
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<String>(
              initialValue: _spaceId,
              decoration: InputDecoration(
                labelText: context.strings.text('encryptedSpace'),
              ),
              items: widget.spaces
                  .map((space) => DropdownMenuItem(
                        value: space.id,
                        child: Text(space.name),
                      ))
                  .toList(),
              onChanged: widget.kindLocked
                  ? null
                  : (value) => setState(() => _spaceId = value ?? _spaceId),
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<PimItemKind>(
              initialValue: _kind,
              decoration: InputDecoration(
                labelText: context.strings.text('itemType'),
              ),
              items: PimItemKind.values
                  .map((kind) => DropdownMenuItem(
                        value: kind,
                        child: Text(context.strings.text(switch (kind) {
                          PimItemKind.calendarEvent => 'calendarEvent',
                          PimItemKind.task => 'task',
                          PimItemKind.contact => 'contact',
                        })),
                      ))
                  .toList(),
              onChanged: widget.kindLocked
                  ? null
                  : (value) => setState(() => _kind = value ?? _kind),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _title,
              autofocus: true,
              maxLength: 500,
              decoration: InputDecoration(
                labelText: _kind == PimItemKind.contact
                    ? context.strings.text('displayName')
                    : context.strings.text('title'),
              ),
            ),
            if (_kind == PimItemKind.task) ..._taskFields(context),
            if (_kind == PimItemKind.calendarEvent) ..._eventFields(context),
            if (_kind == PimItemKind.contact) ..._contactFields(context),
            TextField(
              controller: _notes,
              minLines: 3,
              maxLines: 8,
              decoration: InputDecoration(
                labelText: context.strings.text('notes'),
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _categories,
              decoration: InputDecoration(
                labelText: context.strings.text('categories'),
                helperText: context.strings.text('commaSeparated'),
              ),
            ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(top: 12),
                child: Text(
                  _error!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ),
            const SizedBox(height: 20),
            FilledButton.icon(
              onPressed: _saving ? null : _save,
              icon: _saving
                  ? const SizedBox.square(
                      dimension: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.lock_outline),
              label: Text(context.strings.text('save')),
            ),
          ],
        ),
      );

  List<Widget> _taskFields(BuildContext context) => [
        CheckboxListTile(
          contentPadding: EdgeInsets.zero,
          value: _completed,
          onChanged: (value) => setState(() => _completed = value ?? false),
          title: Text(context.strings.text('completed')),
        ),
        _DateTimeTile(
          title: context.strings.text('due'),
          value: _dueAt,
          onPick: (value) => setState(() => _dueAt = value),
        ),
        DropdownButtonFormField<int>(
          initialValue: _priority,
          decoration: InputDecoration(
            labelText: context.strings.text('priority'),
          ),
          items: [
            DropdownMenuItem(
                value: 0, child: Text(context.strings.text('none'))),
            DropdownMenuItem(
                value: 1, child: Text(context.strings.text('high'))),
            DropdownMenuItem(
                value: 5, child: Text(context.strings.text('medium'))),
            DropdownMenuItem(
                value: 9, child: Text(context.strings.text('low'))),
            if (![0, 1, 5, 9].contains(_priority))
              DropdownMenuItem(
                value: _priority,
                child: Text(
                  '${context.strings.text('importedValue')}: $_priority',
                ),
              ),
          ],
          onChanged: (value) => setState(() => _priority = value ?? 0),
        ),
        const SizedBox(height: 12),
        ..._repeatAndReminder(context, includeRepeat: false),
      ];

  List<Widget> _eventFields(BuildContext context) => [
        SwitchListTile.adaptive(
          contentPadding: EdgeInsets.zero,
          title: Text(context.strings.text('allDay')),
          value: _allDay,
          onChanged: (value) => setState(() {
            _allDay = value;
            _endTouched = true;
          }),
        ),
        _DateTimeTile(
          title: context.strings.text('starts'),
          value: _startsAt,
          dateOnly: _allDay,
          onPick: (value) => setState(() => _startsAt = value),
        ),
        _DateTimeTile(
          title: context.strings.text(
            _allDay ? 'endsInclusive' : 'ends',
          ),
          value: _endsAt,
          dateOnly: _allDay,
          onPick: (value) => setState(() {
            _endsAt = value;
            _endTouched = true;
          }),
        ),
        TextField(
          controller: _location,
          decoration:
              InputDecoration(labelText: context.strings.text('location')),
        ),
        const SizedBox(height: 12),
        ..._repeatAndReminder(context),
      ];

  List<Widget> _repeatAndReminder(
    BuildContext context, {
    bool includeRepeat = true,
  }) =>
      [
        if (includeRepeat)
          DropdownButtonFormField<String>(
            initialValue: _recurrence,
            decoration:
                InputDecoration(labelText: context.strings.text('repeat')),
            items: [
              DropdownMenuItem(
                  value: '', child: Text(context.strings.text('never'))),
              DropdownMenuItem(
                  value: 'FREQ=DAILY',
                  child: Text(context.strings.text('daily'))),
              DropdownMenuItem(
                  value: 'FREQ=WEEKLY',
                  child: Text(context.strings.text('weekly'))),
              DropdownMenuItem(
                  value: 'FREQ=MONTHLY',
                  child: Text(context.strings.text('monthly'))),
              DropdownMenuItem(
                  value: 'FREQ=YEARLY',
                  child: Text(context.strings.text('yearly'))),
              if (![
                '',
                'FREQ=DAILY',
                'FREQ=WEEKLY',
                'FREQ=MONTHLY',
                'FREQ=YEARLY',
              ].contains(_recurrence))
                DropdownMenuItem(
                  value: _recurrence,
                  child: Text(
                    '${context.strings.text('importedValue')}: $_recurrence',
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
            ],
            onChanged: (value) => setState(() => _recurrence = value ?? ''),
          ),
        if (includeRepeat) const SizedBox(height: 12),
        DropdownButtonFormField<String>(
          initialValue: _reminder,
          decoration:
              InputDecoration(labelText: context.strings.text('reminder')),
          items: [
            DropdownMenuItem(
                value: '', child: Text(context.strings.text('none'))),
            DropdownMenuItem(
                value: '0', child: Text(context.strings.text('atTime'))),
            DropdownMenuItem(
                value: '10',
                child: Text(context.strings.text('tenMinutesBefore'))),
            DropdownMenuItem(
                value: '60',
                child: Text(context.strings.text('oneHourBefore'))),
            DropdownMenuItem(
                value: '1440',
                child: Text(context.strings.text('oneDayBefore'))),
            if (!['', '0', '10', '60', '1440'].contains(_reminder))
              DropdownMenuItem(
                value: _reminder,
                child: Text(
                  '${context.strings.text('importedValue')}: $_reminder',
                ),
              ),
          ],
          onChanged: (value) => setState(() => _reminder = value ?? ''),
        ),
        const SizedBox(height: 12),
      ];

  List<Widget> _contactFields(BuildContext context) => [
        Row(
          children: [
            Expanded(
              child: TextField(
                controller: _prefix,
                decoration: InputDecoration(
                  labelText: context.strings.text('namePrefix'),
                ),
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: TextField(
                controller: _suffix,
                decoration: InputDecoration(
                  labelText: context.strings.text('nameSuffix'),
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: 12),
        Row(
          children: [
            Expanded(
              child: TextField(
                controller: _given,
                decoration: InputDecoration(
                    labelText: context.strings.text('givenName')),
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: TextField(
                controller: _family,
                decoration: InputDecoration(
                    labelText: context.strings.text('familyName')),
              ),
            ),
          ],
        ),
        const SizedBox(height: 12),
        TextField(
          controller: _middle,
          decoration: InputDecoration(
            labelText: context.strings.text('middleName'),
          ),
        ),
        const SizedBox(height: 12),
        Row(
          children: [
            Expanded(
              child: TextField(
                controller: _organization,
                decoration: InputDecoration(
                    labelText: context.strings.text('organization')),
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: TextField(
                controller: _jobTitle,
                decoration: InputDecoration(
                    labelText: context.strings.text('jobTitle')),
              ),
            ),
          ],
        ),
        const SizedBox(height: 12),
        TextField(
          controller: _emails,
          keyboardType: TextInputType.emailAddress,
          minLines: 1,
          maxLines: 4,
          decoration: InputDecoration(
            labelText: context.strings.text('emailAddresses'),
            helperText: context.strings.text('onePerLineWithLabel'),
          ),
        ),
        const SizedBox(height: 12),
        TextField(
          controller: _phones,
          keyboardType: TextInputType.phone,
          minLines: 1,
          maxLines: 4,
          decoration: InputDecoration(
            labelText: context.strings.text('phoneNumbers'),
            helperText: context.strings.text('onePerLineWithLabel'),
          ),
        ),
        const SizedBox(height: 12),
        Align(
          alignment: Alignment.centerLeft,
          child: TextButton.icon(
            onPressed: () => setState(
              () => _addresses = [..._addresses, _AddressControllers()],
            ),
            icon: const Icon(Icons.add_location_alt_outlined),
            label: Text(context.strings.text('addAddress')),
          ),
        ),
        ..._addresses.asMap().entries.expand((entry) {
          final index = entry.key;
          final address = entry.value;
          return [
            Card.outlined(
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Column(
                  children: [
                    Row(
                      children: [
                        Expanded(
                          child: TextField(
                            controller: address.label,
                            decoration: InputDecoration(
                              labelText: context.strings.text('label'),
                            ),
                          ),
                        ),
                        IconButton(
                          tooltip: context.strings.text('removeAddress'),
                          onPressed: () {
                            setState(() => _addresses.removeAt(index));
                            address.dispose();
                          },
                          icon: const Icon(Icons.delete_outline),
                        ),
                      ],
                    ),
                    TextField(
                      controller: address.street,
                      decoration: InputDecoration(
                        labelText: context.strings.text('street'),
                      ),
                    ),
                    Row(
                      children: [
                        Expanded(
                          child: TextField(
                            controller: address.locality,
                            decoration: InputDecoration(
                              labelText: context.strings.text('city'),
                            ),
                          ),
                        ),
                        const SizedBox(width: 12),
                        Expanded(
                          child: TextField(
                            controller: address.region,
                            decoration: InputDecoration(
                              labelText: context.strings.text('region'),
                            ),
                          ),
                        ),
                      ],
                    ),
                    Row(
                      children: [
                        Expanded(
                          child: TextField(
                            controller: address.postalCode,
                            decoration: InputDecoration(
                              labelText: context.strings.text('postalCode'),
                            ),
                          ),
                        ),
                        const SizedBox(width: 12),
                        Expanded(
                          child: TextField(
                            controller: address.country,
                            decoration: InputDecoration(
                              labelText: context.strings.text('country'),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 8),
          ];
        }),
        const SizedBox(height: 12),
        Row(
          children: [
            Expanded(
              child: TextField(
                controller: _birthday,
                keyboardType: TextInputType.datetime,
                decoration: InputDecoration(
                  labelText: context.strings.text('birthday'),
                  hintText: 'YYYY-MM-DD',
                ),
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: TextField(
                controller: _url,
                keyboardType: TextInputType.url,
                decoration:
                    InputDecoration(labelText: context.strings.text('website')),
              ),
            ),
          ],
        ),
        SwitchListTile.adaptive(
          contentPadding: EdgeInsets.zero,
          title: Text(context.strings.text('favorite')),
          value: _favorite,
          onChanged: (value) => setState(() => _favorite = value),
        ),
      ];

  Future<void> _save() async {
    final title = _title.text.trim().isNotEmpty
        ? _title.text.trim()
        : '${_given.text.trim()} ${_family.text.trim()}'.trim();
    if (title.isEmpty) {
      setState(() => _error = context.strings.text('titleRequired'));
      return;
    }
    final invalidEventRange = _kind == PimItemKind.calendarEvent &&
        (_startsAt == null ||
            (_endsAt == null && !(_endWasMissing && !_endTouched)) ||
            (_allDay
                ? _endsAt != null && _endsAt!.isBefore(_startsAt!)
                : _endsAt != null && !_endsAt!.isAfter(_startsAt!)));
    if (invalidEventRange) {
      setState(() => _error = context.strings.text('validEventRange'));
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    final initial = widget.initial;
    final saved = await widget.onSave(PimDraft(
      spaceId: _spaceId,
      resourceId: initial.resourceId,
      projectionId: initial.projectionId,
      headOperationId: initial.headOperationId,
      kind: _kind,
      title: title,
      completed: _completed,
      completedAt: _completed
          ? initial.completedAt ?? _formatIcalUtc(DateTime.now())
          : null,
      notes: _emptyToNull(_notes.text),
      startsAt: _startsAt == null
          ? null
          : _allDay
              ? PimTemporal.allDay(_startsAt!)
              : _timedTemporal(_startsAt!, initial.startsAt),
      endsAt: _endWasMissing && !_endTouched
          ? null
          : _endsAt == null
              ? null
              : _allDay
                  ? PimTemporal.allDay(_endsAt!.add(const Duration(days: 1)))
                  : _timedTemporal(_endsAt!, initial.endsAt),
      dueAt: _dueAt == null ? null : _timedTemporal(_dueAt!, initial.dueAt),
      priority: _priority,
      location: _emptyToNull(_location.text),
      recurrenceRule: _emptyToNull(_recurrence),
      reminderMinutes: int.tryParse(_reminder),
      categories: _categories.text
          .split(',')
          .map((value) => value.trim())
          .where((value) => value.isNotEmpty)
          .toList(),
      namePrefix: _prefix.text.trim(),
      givenName: _given.text.trim(),
      middleName: _middle.text.trim(),
      familyName: _family.text.trim(),
      nameSuffix: _suffix.text.trim(),
      emails: _preserveLabeledHeads(
        _parseLabeled(_emails.text, defaultLabel: 'home'),
        initial.emails,
      ),
      phones: _preserveLabeledHeads(
        _parseLabeled(_phones.text, defaultLabel: 'mobile'),
        initial.phones,
      ),
      addresses: _addresses
          .where((address) => !address.isEmpty)
          .map((address) => address.toValue())
          .toList(),
      organization: _emptyToNull(_organization.text),
      jobTitle: _emptyToNull(_jobTitle.text),
      birthday: _emptyToNull(_birthday.text),
      url: _emptyToNull(_url.text),
      favorite: _favorite,
    ));
    if (!mounted) return;
    if (saved) {
      Navigator.pop(context);
    } else {
      setState(() {
        _saving = false;
        _error = context.strings.text('saveFailed');
      });
    }
  }

  PimTemporal _timedTemporal(DateTime value, PimTemporal? original) {
    final originalDate = original?.toEditorDateTime();
    if (original != null &&
        originalDate != null &&
        value.isAtSameMomentAs(originalDate)) {
      return original;
    }
    if (original?.kind != 'zoned_datetime' || original?.timezone == null) {
      return PimTemporal.utc(value);
    }
    return PimTemporal.zoned(value, original!.timezone!);
  }

  List<PimLabeledValue> _preserveLabeledHeads(
    List<PimLabeledValue> values,
    List<PimLabeledValue> originals,
  ) =>
      values.indexed.map((entry) {
        final index = entry.$1;
        final value = entry.$2;
        final original = index < originals.length ? originals[index] : null;
        return PimLabeledValue(
          label: value.label,
          value: value.value,
          rawHead: original?.label == value.label ? original?.rawHead : null,
        );
      }).toList();
}

class _DateTimeTile extends StatelessWidget {
  const _DateTimeTile({
    required this.title,
    required this.value,
    required this.onPick,
    this.dateOnly = false,
  });

  final String title;
  final DateTime? value;
  final ValueChanged<DateTime?> onPick;
  final bool dateOnly;

  @override
  Widget build(BuildContext context) => ListTile(
        contentPadding: EdgeInsets.zero,
        title: Text(title),
        subtitle: Text(value == null
            ? context.strings.text('notSet')
            : _displayDateTime(value!, dateOnly: dateOnly)),
        trailing: Wrap(
          children: [
            if (value != null)
              IconButton(
                onPressed: () => onPick(null),
                icon: const Icon(Icons.clear),
              ),
            IconButton(
              onPressed: () async {
                final picked =
                    await _pickDateTime(context, value, dateOnly: dateOnly);
                if (picked != null) onPick(picked);
              },
              icon: const Icon(Icons.edit_calendar_outlined),
            ),
          ],
        ),
      );
}

class _EmptyOrganizer extends StatelessWidget {
  const _EmptyOrganizer({required this.kind});

  final PimItemKind? kind;

  @override
  Widget build(BuildContext context) => Center(
        child: Padding(
          padding: const EdgeInsets.all(32),
          child: Text(
            context.strings.text(kind == PimItemKind.task
                ? 'emptyTasks'
                : kind == PimItemKind.calendarEvent
                    ? 'emptyEvents'
                    : kind == PimItemKind.contact
                        ? 'emptyContacts'
                        : 'emptyOrganizer'),
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
      PimItemKind.contact => [
          item.organization,
          item.emails.firstOrNull?.value,
          item.phones.firstOrNull?.value,
        ].whereType<String>().where((value) => value.isNotEmpty).join(' · '),
      PimItemKind.calendarEvent => [
          _displayTemporal(item.startsAt),
          item.location,
        ].whereType<String>().where((value) => value.isNotEmpty).join(' · '),
      PimItemKind.task => item.completed
          ? context.strings.text('completed')
          : item.dueAt == null
              ? context.strings.text('open')
              : '${context.strings.text('due')}: ${_displayTemporal(item.dueAt)}',
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
                PimItemKind.contact =>
                  item.favorite ? Icons.star : Icons.person_outline,
              }),
        title: Text(
          item.title,
          style: item.completed
              ? const TextStyle(decoration: TextDecoration.lineThrough)
              : null,
        ),
        subtitle: subtitle.isEmpty
            ? null
            : Text(item.conflict
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

Future<DateTime?> _pickDateTime(
  BuildContext context,
  DateTime? initial, {
  bool dateOnly = false,
}) async {
  final seed = initial?.toLocal() ?? DateTime.now();
  final date = await showDatePicker(
    context: context,
    firstDate: DateTime(1970),
    lastDate: DateTime(2100),
    initialDate: seed,
  );
  if (date == null || !context.mounted || dateOnly) return date;
  final time = await showTimePicker(
    context: context,
    initialTime: TimeOfDay.fromDateTime(seed),
  );
  if (time == null) return null;
  return DateTime(date.year, date.month, date.day, time.hour, time.minute);
}

String _formatIcalUtc(DateTime value) {
  final utc = value.toUtc();
  String two(int number) => number.toString().padLeft(2, '0');
  return '${utc.year}${two(utc.month)}${two(utc.day)}T${two(utc.hour)}${two(utc.minute)}${two(utc.second)}Z';
}

String _displayTemporal(PimTemporal? value) {
  final date = value?.toDateTime();
  return date == null ? '' : _displayDateTime(date, dateOnly: value!.isAllDay);
}

String _displayDateTime(DateTime value, {bool dateOnly = false}) {
  final local = value.toLocal();
  String two(int number) => number.toString().padLeft(2, '0');
  final date = '${local.year}-${two(local.month)}-${two(local.day)}';
  return dateOnly ? date : '$date ${two(local.hour)}:${two(local.minute)}';
}

String? _emptyToNull(String value) =>
    value.trim().isEmpty ? null : value.trim();

String _formatLabeled(List<PimLabeledValue> values) =>
    values.map((value) => '${value.label}: ${value.value}').join('\n');

List<PimLabeledValue> _parseLabeled(
  String value, {
  required String defaultLabel,
}) =>
    value
        .split('\n')
        .map((line) => line.trim())
        .where((line) => line.isNotEmpty)
        .map((line) {
      final separator = line.indexOf(': ');
      return separator > 0
          ? PimLabeledValue(
              label: line.substring(0, separator).trim(),
              value: line.substring(separator + 2).trim(),
            )
          : PimLabeledValue(label: defaultLabel, value: line);
    }).toList();

extension<T> on List<T> {
  T? get firstOrNull => isEmpty ? null : first;
}
