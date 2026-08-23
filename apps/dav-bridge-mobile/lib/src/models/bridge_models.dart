/// Result of an authentication attempt.
class LoginResult {
  const LoginResult({
    this.username,
    this.accessToken,
    this.totpContinuationToken,
    this.deviceEnrollmentToken,
    required this.totpVerified,
    this.accountMasterKey,
  });

  final String? username;
  final String? accessToken;
  final String? totpContinuationToken;
  final String? deviceEnrollmentToken;
  final bool totpVerified;
  final List<int>? accountMasterKey;
}

class DeviceSecrets {
  const DeviceSecrets({
    required this.deviceId,
    required this.signingPrivateKey,
    required this.hpkePrivateKey,
    required this.hpkePublicKey,
  });

  final String deviceId;
  final List<int> signingPrivateKey;
  final List<int> hpkePrivateKey;
  final List<int> hpkePublicKey;
}

class ProvisionResult {
  const ProvisionResult({
    required this.accessToken,
    required this.device,
    required this.collections,
  });

  final String accessToken;
  final DeviceSecrets device;
  final List<CollectionEntry> collections;
}

/// Result of issuing a short-lived collection invite code.
class IssuedInviteCode {
  const IssuedInviteCode({
    required this.code,
    required this.ttlMinutes,
    required this.keyEpoch,
    required this.currentStateStartSeq,
    required this.collectionKey,
  });

  final String code;
  final int ttlMinutes;
  final int keyEpoch;
  final int currentStateStartSeq;
  final List<int> collectionKey;
}

/// Result of redeeming an invite code.
class RedeemedInvite {
  const RedeemedInvite({
    required this.collectionId,
    required this.role,
    required this.keyEpoch,
    this.historyStartSeq = 0,
    this.currentStateStartSeq = 0,
    required this.collectionKey,
  });

  final String collectionId;
  final String role;
  final int keyEpoch;
  final int historyStartSeq;
  final int currentStateStartSeq;
  final List<int> collectionKey;
}

/// In-memory collection descriptor used by the mobile bridge UI.
class CollectionEntry {
  const CollectionEntry({
    required this.id,
    required this.name,
    required this.cmk,
    this.keyEpoch = 1,
    this.historyStartSeq = 0,
    this.currentStateStartSeq = 0,
    this.role = 'owner',
  });

  final String id;
  final String name;
  final List<int> cmk;
  final int keyEpoch;
  final int historyStartSeq;
  final int currentStateStartSeq;
  final String role;
}

enum PimItemKind {
  calendarEvent('calendar_event'),
  task('task'),
  contact('contact');

  const PimItemKind(this.wireName);

  final String wireName;

  static PimItemKind fromWireName(String value) => switch (value) {
        'calendar_event' => PimItemKind.calendarEvent,
        'task' => PimItemKind.task,
        'contact' => PimItemKind.contact,
        _ => throw FormatException('Unknown PIM item kind: $value'),
      };
}

class PimItem {
  const PimItem({
    required this.spaceId,
    required this.resourceId,
    required this.projectionId,
    required this.headOperationId,
    required this.kind,
    required this.title,
    required this.completed,
    this.email,
    this.phone,
    this.startsAt,
    this.endsAt,
    this.conflict = false,
  });

  final String spaceId;
  final String resourceId;
  final String projectionId;
  final String headOperationId;
  final PimItemKind kind;
  final String title;
  final bool completed;
  final String? email;
  final String? phone;
  final String? startsAt;
  final String? endsAt;
  final bool conflict;
}
