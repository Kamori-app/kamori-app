### Spec Compliance

- ❌ Issues found: the local-only engine contract is not enforced by the default Podman wrapper. `tests/release-readiness/restore/run.py:20` chooses that wrapper and `run.py:23` only sets `DOCKER_HOST`; inherited Podman endpoint selectors can redirect the entire exercise. See Important I1.
- ✅ The requested substantive exercise is implemented: checksummed PostgreSQL 16 and encrypted archive configuration (`tests/release-readiness/restore/pg-setup.sh:8`, `:22`, `:27`); real SQLx migration execution/source checksum validation, full backup plus production job, distinct recovery volume and exact data/schema comparisons (`tests/release-readiness/restore/run.py:150`, database function); independent MinIO stores, production replication, credential denial, stopped-primary reads and repaired missing/corrupt negative controls (`tests/release-readiness/restore/blob-exercise.sh:17`, `:31`, `:32`, `:41`); redacted measured evidence (`tests/release-readiness/restore/evidence-2026-10-15.json:1`). No production script changes or unrelated product behavior appear in this task diff.
- ✅ The reproduced write-only credential contradiction is corrected, including the externally supplied credential limitation (`docs/runbooks/backup-restore.md:14`, `:30`). The harness and runbook explicitly separate local correctness from actual provider permissions, production RPO/RTO and application restore gates (`tests/release-readiness/restore/README.md:52`, `docs/runbooks/backup-restore.md:117`).
- ⚠️ Cannot independently establish historical runtime results or unchanged deployment policy from this diff. The implementation report and committed evidence record actual PostgreSQL 16 recovery, 23 SQLx migrations, zero bad page checksums, restored ciphertext/accounting, production job heartbeats, all three DR objects, negative controls, fresh Go 1.25.11 tests/vet, scoped shell checks and empty cleanup inventories. These are accepted as the reported execution evidence, not rerun. Controller owns final integration and remote/production gates. Global application constraints and preservation of the other thirty fixes are outside this task diff; no application code changes appear here.

### Strengths

- `tests/release-readiness/restore/pg-setup.sh:22` enables physical checksums before backup; `run.py:150` performs a genuine encrypted time-target restore and compares restored database manifests and schema, rather than accepting backup command success.
- `tests/release-readiness/restore/blob-exercise.sh:32` checks both byte length and SHA-256, and `:41` supplies concrete missing and same-length corruption cases. The runner derives the read manifest from recovered database metadata and leaves the primary stopped during DR reads (run.py, blobs function).
- `tests/release-readiness/restore/run.py:101` rechecks both ownership labels before removal, preserves cleanup errors and verifies successful inventories. `run.py:128` rejects existing containers/volumes before adoption or creation. No global prune, host PGDATA mutation or shared-stack operation appears.
- `tests/release-readiness/restore/Dockerfile:1` pins both base image digests and direct package versions. `tests/release-readiness/restore/README.md:23` accurately identifies AES-CBC and limits the wrong-key assertion to repository decoding failure.

### Issues

#### Critical (Must Fix)

- None found.

#### Important (Should Fix)

- **I1 — the promised fixed local endpoint is not actually selected.** `tests/release-readiness/restore/run.py:20`, `:22`, `:23`, `:53`: the default `/Users/yujin/.local/bin/docker` is a shell wrapper containing `exec podman "$@"`. `ENV = dict(os.environ, DOCKER_HOST=SOCKET)` preserves `CONTAINER_HOST` and `CONTAINER_CONNECTION`, while the Podman CLI uses its own connection configuration instead of `DOCKER_HOST`. Therefore an inherited remote Podman connection can receive the harness's network/volume/container creation, synthetic trust-authenticated database and cleanup operations, contrary to the explicit local-only/no-production-service constraint. Current successful local evidence does not establish this boundary. Bind the actual backend explicitly to the reserved Unix socket (for example Podman `--url`, or a verified Docker-compatible `--host` adapter), remove conflicting endpoint selectors, and reject unsupported adapters instead of trusting an arbitrary environment override. Add focused endpoint-selection regression evidence without contacting remote services.
  - Focused unchanged-file lookup for this named cross-file risk: inspected only `/Users/yujin/.local/bin/docker:1` and its local `--help`; the wrapper calls Podman and help names `CONTAINER_HOST`/`CONTAINER_CONNECTION`.
  - Read-only probe: `env CONTAINER_HOST=unix:///private/tmp/kamori-review-no-daemon.sock DOCKER_HOST=unix:///private/tmp/podman/podman-machine-default-api.sock /Users/yujin/.local/bin/docker version` failed with `dial unix /private/tmp/kamori-review-no-daemon.sock: connect: no such file or directory`. This proves the harness's assigned `DOCKER_HOST` does not enforce its endpoint. The probe contacted no daemon and mutated no resource.

#### Minor (Nice to Have)

- **M1 — partial container creation is not included in cleanup accounting.** `tests/release-readiness/restore/run.py:96` records a container in `created` only after `docker run` succeeds (`:97`). If the daemon creates the container but starting it fails, or the client times out/is interrupted after creation, `cleanup()` iterates and inventories only the existing `created` list (`:103`, `:120`), so it can omit that owned container from both removal and `cleanup.json`. This does not invalidate the reported successful cleanup, and the original exception prevents PASS, but it weakens failure recovery evidence. Track the attempted exact name and reconcile its existence/ownership after failed creation, removing it only after the same label checks. This is a code-path observation, not a reproduced daemon failure.

### Assessment

**Task quality:** Needs fixes.

**Reasoning:** The real restore and integrity checks satisfy the substantive task and the evidence is carefully bounded. The endpoint-selection flaw must be fixed before this destructive reusable harness can be trusted to stay within its authorized local environment.

**Review scope/checks:** Reviewed `7de8436c110c7744c695e4be72f0b8b1def796a6..0b1cc7fd6960e01241f8ab7ea5ed66fd86238b5d` using the supplied diff, brief, context and implementer report. The first tool response truncated the middle of the diff; recovered only the affected evidence/pg-setup/run.py opening region from that same diff. No changed-file rereads, git rederivation, broad codebase crawl, suite reruns, branch/index mutations, subagents or shared-resource operations. The only focused executable probe was the endpoint selection check above. Reported existing ShellCheck exclusions are explicitly scoped/documented on the unchanged production job; no new unexplained warning was identified.
