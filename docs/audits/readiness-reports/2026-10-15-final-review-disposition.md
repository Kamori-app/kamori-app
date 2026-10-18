# Disposition of final-review exclusions

[Source review](2026-10-15-final-review.md), HEAD `cdca230`: all three concrete findings C1/R16, I1 and M1 enter one correction wave; none is parked. Controller judgments for all twelve declined behaviors:

1. Generic cancellation/session runtime: fix actual lifetime/scope writes with existing coordination; no new runtime is necessary. Preserve current-lifetime acknowledgements/retries (decision8, Global Constraints).
2. Agenda default anchor: retain selected anchor, visible range and Today per decision13. Fix the test's week selection; no separate demonstrated product bug.
3. Every durability case on PR: retain accepted smoke/full split. Decision20 adds the NEW critical regression to @smoke; extra browser execution is the cost.
4. Physical providers/scheduler/secure store/data protection/power loss: external device/fault exercises under decision3. Current simulator/process evidence does not certify them.
5. Signing/TestFlight/store/Android runtime: external gates under decision3; current results are unsigned arm64 and three packaged ELF ABIs.
6. Remote IAM/TLS/retention/network/RPO/RTO and restored-app auth/sync/export: explicit runbook exercises under decisions4/5. PG16/MinIO storage canaries are not signed app messages or remote configuration certification.
7. Arbitrary SIGKILL/host failure during provisioning: no automatic crash-recovery claim. Ownership/collision controls and documented manual recovery preserve safety; a killed process can leave its resources/lock. Actual partial-start controls pass, no remaining demonstrated production script defect.
8. Every DAV client/arbitrary recurrence/advanced authoring: retain supported corpus/raw preservation and explicit PERIOD/RANGE/native-projection limits under decisions11/12/14; broader matrix remains external.
9. Remote GitHub/Ubuntu and another pristine native checkout: do not claim unexecuted environments. Local CI input selection/static inputs/native builds verified; decisions3/6 retain environment-drift cost.
10. Future Flutter/SPM/generator/warning-free tools: track SPM migration before Flutter upgrade. Current pinned functional gates pass; no speculative upgrade or diagnostic suppression.
11. New numerical coverage/exhaustive pre-baseline audit: historical revision-qualified snapshots and original30-fix reviews remain provenance. No new percentage measured, tested schedules finite per decision2.
12. Production/legal release/independent security certification: outside authorized local work. No deployment/legal acceptance/signing submission/certification; local branch delivery per decision1/Global Constraints.

No exclusion dismisses the proven cross-account write, failed smoke or inaccurate marker. Scoped review and fresh full acceptance must close all three; physical/remote/distribution gates remain visible afterward.

Completed: C1/I1/M1 addressed at b85f70c; subsequent MinorN1 addressed at e3c86d5 under decision21, scoped round2 clean. Final current-source acceptance45/45/no retries and exact owned-resource cleanup PASS. All twelve exclusions above remain explicit release/proof boundaries, no confirmed defect was parked.
