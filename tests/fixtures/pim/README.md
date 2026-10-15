# Synthetic imported PIM corpus

Authored for Kamori tests; no exported user data, third-party fixture licensing,
network services, credentials, or real contact addresses. Files intentionally
use CRLF and retain property parameters, folding, and component order.

`corpus.json` contains independently authored expected canonical field subsets.
Rust checks them through `projection_fields`; web reads the same source files;
mobile consumes these checked typed fields at its existing model boundary.

The zoned calendar places a detached override before its master, crosses the
2026 New York DST transition, and includes EXDATE, RDATE, folded custom values,
a custom RRULE, a 17-minute Kamori alarm and separate external alarms. The
all-day calendar has an exclusive end, date-valued exceptions and an external
alarm. The imported task preserves an unauthored monthly rule and priority 4.
The contact preserves a grouped email with a custom quoted label.

Display expectations cover the union of DTSTART/RRULE/RDATE, EXDATE removal,
original RECURRENCE-ID matching, a moved override and cancellation, duplicate
RDATE suppression, COUNT, DST, and exclusive all-day ends. Timed days follow the
viewer timezone; all-day values remain civil calendar dates. Metadata is derived
for display only; the encrypted operation format and raw projections remain
authoritative. The UI edits/deletes the original series, not a detached override.

The fifth fixture contains RDATE PERIOD and RECURRENCE-ID RANGE. These values
remain byte-preserved; expansion is explicitly unsupported and both clients show
a notice instead of a misleading occurrence list. This is a bounded compatibility
corpus, not full RFC 5545 certification. Task recurrence and advanced exception
**authoring** remain post-MVP per SPEC §7. Imported RRULEs, priority 4, labels and
17-minute reminders must not be normalized merely by opening a draft.

The pinned native Calendar provider exposes RRULE-only recurrence writes, so
optional OS Calendar projection refuses imported exception sets explicitly. Any
previous owned mapped copy is removed after calendar/marker verification, and
its mapping is durably cleared only after provider success. Provider failures
retain the mapping; a successful delete followed by storage failure is retried
safely on reconciliation. Original encrypted data and other collections remain
untouched; no permission prompt is requested for the refusal path.
