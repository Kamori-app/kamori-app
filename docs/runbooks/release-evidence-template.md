# Release evidence template

Copy this template into the private operator evidence store for every client
release. Do not commit completed reports: they may contain infrastructure
details, account identifiers, or security evidence. A public release may link
to a separately prepared redacted summary.

Use `Pass`, `Fail`, `Blocked`, or `Not applicable` for every result. A release
must remain a GitHub draft while a required gate is not `Pass`.

## Release identity

- Version/tag:
- Release commit SHA:
- Signed-tag verification result:
- GitHub Actions run URL:
- Draft release URL:
- Operator and UTC date:
- Released channels:
- Explicitly excluded channels and reason:

## Automated repository gates

- Required CI checks:
- Manifest/tag version check:
- Artifact checksums captured from the workflow:
- Dependency and license review report:
- Result and evidence location:

## PostgreSQL PITR exercise

- Disposable isolated restore target:
- Backup base and target recovery time:
- Restore start/end UTC and measured RTO:
- Latest recoverable transaction UTC and measured RPO:
- Schema/application integrity checks:
- Confirmation that production was not modified:
- Result and private evidence location:

## Ciphertext object-store recovery

- Disaster-recovery source and isolated destination:
- Sample/object count and total bytes:
- Ciphertext size/hash verification:
- Confirmation that the primary object store was not read during the drill:
- Result and private evidence location:

## Operations and alert delivery

- Expected backup/cleanup job heartbeats:
- Expected Prometheus targets:
- Synthetic Alertmanager notification destination and receipt time:
- External monitoring checks for API, web, admin, DNS, and TLS:
- Result and private evidence location:

External availability monitoring is a separate service. Never implement or
run production probes from CI/CD as release evidence.

## Desktop DAV compatibility

| OS | DAV client/version | Calendar | Tasks | Contacts | Offline/resync | Result/evidence |
| --- | --- | --- | --- | --- | --- | --- |
| | | | | | | |

## Signed client artifacts

| Channel/artifact | Signature verification | Clean install/update | Sign-in/recovery | Offline/sync | DAV or projection | Result/evidence |
| --- | --- | --- | --- | --- | --- | --- |
| Linux AppImage/deb/rpm | | | | | Desktop DAV | |
| Flatpak/repository | | | | | Desktop DAV | |
| macOS | | | | | Desktop DAV | |
| Windows | | | | | Desktop DAV | |
| Android universal APK | | | | | System projection | |
| Android Accrescent APKS | | | | | System projection | |
| Android Play AAB | | | | | System projection | |
| iOS TestFlight (only when enabled) | | | | | System projection | |

For every mobile channel that is being released, exercise a physical supported
device. Record iOS as `Not applicable — channel disabled` until the iOS release
path is explicitly enabled; do not silently omit it.

## Product and legal checklist

- Intended production API origin verified in every artifact:
- Mock/development switches absent:
- Registration and quotas match the announced release policy:
- Privacy policy, terms, license notices, and store disclosures reviewed:
- Legal-template owner/operator fields completed and approved:
- Result and private evidence location:

## Decision

- Final decision: Publish / Hold
- Approver and UTC date:
- Known limitations included in release notes:
- Blocking issues and owners:
