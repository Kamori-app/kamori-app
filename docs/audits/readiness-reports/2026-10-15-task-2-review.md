### Spec Compliance

- ✅ Spec compliant for Task 2, base `85a14ca70c9ae60b6424ce08c6100e2d7f87dfc6` to head `d4d9fd77a4088d504dcceb77a6f3f08ef893a8c0`. The existing shared test helper now requires a nonblank explicit URL or fallback and emits actionable missing-PostgreSQL diagnostics (`apps/cloud-server/src/platform/test_support.rs:14`). Pure resolution tests cover precedence, fallback, absent and blank combinations without mutating the process environment (`apps/cloud-server/src/platform/test_support.rs:123`).
- ✅ The representative subprocess runs the real admin integration test with both URL variables removed only in the child, asserts exactly one test and its name, checks nonzero exit, and checks the prerequisite diagnostic (`apps/cloud-server/src/platform/test_support.rs:170`). The saved RED log shows that the old helper passed the selected child test after printing its skip message; GREEN evidence establishes the intended behavior change (`/private/tmp/kamori-readiness-20261015/db-tests-red.log:327`, `/private/tmp/kamori-readiness-20261015/db-tests-green-prerequisite.log:12`).
- ✅ Auth, admin, invites, operations and nested operations lock-regression helpers/callers all have corresponding hunks removing optional setup and silent successful returns (`apps/cloud-server/src/features/auth/handlers/integration_tests.rs:104`, `apps/cloud-server/src/features/admin/integration_tests.rs:17`, `apps/cloud-server/src/features/invites/integration_tests.rs:18`, `apps/cloud-server/src/features/operations/integration_tests.rs:22`, `apps/cloud-server/src/features/operations/lock_regressions.rs:503`). The diff retains existing migrations, pool limits, schema isolation, lock staging and behavioral assertions.
- ✅ Full-suite prerequisites and filtered unit-only commands are documented (`apps/cloud-server/README.md:438`, `README.md:62`). No dependency, script or CI edit is needed: the existing Rust CI job supplies PostgreSQL 18 and DATABASE_URL before running workspace tests (`.github/workflows/ci.yml:75`, `.github/workflows/ci.yml:89`, `.github/workflows/ci.yml:136`).
- ⚠️ Cannot verify from this task diff alone: the continued behavior of all thirty earlier production fixes and every cross-task release requirement. This diff changes test setup and documentation, with no production logic or lock coordination hunks; the controller retains the broader branch/release verification gate.

### Strengths

- The resolver is a small pure function, with environment access confined to its wrapper; whitespace-only values correctly fall through without exposing URL contents (`apps/cloud-server/src/platform/test_support.rs:14`, `apps/cloud-server/src/platform/test_support.rs:26`).
- The subprocess regression protects against accidentally selecting zero tests and proves a real database test fails at setup, rather than merely testing the resolver in isolation (`apps/cloud-server/src/platform/test_support.rs:170`).
- Existing unit test helpers remain lazy, and saved evidence shows two selected invite-service unit tests passing without either DB variable (`apps/cloud-server/src/platform/test_support.rs:115`, `/private/tmp/kamori-readiness-20261015/db-tests-unit-only.log:8`).

### Issues

#### Critical (Must Fix)

- None found.

#### Important (Should Fix)

- None found.

#### Minor (Nice to Have)

- None found.

### Checks and Evidence

- Reviewed the supplied diff; the first combined output was truncated, so retrieved only the missing diff sections in bounded slices. No changed source file was opened separately, and no git commands, suites, source/index/ref mutations or subagents were used.
- Named risk: an unchanged database helper might retain a silent skip after centralization. Ran one focused source search for DB environment names, missing-DB skip messages and optional pool/app helper signatures. Results identified the shared test helper and unchanged production configuration/bootstrap reads, with no remaining optional DB test helper or skip-message match outside the converted helpers (`apps/cloud-server/src/app/bootstrap.rs:63`, `apps/cloud-server/src/app/bootstrap.rs:84`, `apps/cloud-server/src/platform/config.rs:306`).
- Named risk: making DB configuration mandatory could break existing CI. Checked the existing workflow's PostgreSQL service, database environment and test invocation; it already meets the prerequisite (`.github/workflows/ci.yml:75`, `.github/workflows/ci.yml:89`, `.github/workflows/ci.yml:136`). No workflow or shell changes occurred, so detector/shell validation is not applicable to this task.
- Inspected saved verification logs, without rerunning tests. Confirmed RED child success/outer failure, five prerequisite tests GREEN, one real-DB fallback test GREEN, two unit-only tests GREEN, and the direct missing-DB test's intentional failure (`/private/tmp/kamori-readiness-20261015/db-tests-red.log:332`, `/private/tmp/kamori-readiness-20261015/db-tests-green-prerequisite.log:12`, `/private/tmp/kamori-readiness-20261015/db-tests-fallback-real-db.log:7`, `/private/tmp/kamori-readiness-20261015/db-tests-unit-only.log:8`, `/private/tmp/kamori-readiness-20261015/db-tests-missing-db-intentional-failure.log:8`).
- The full-server log records 156 passed, zero failed/ignored/filtered, including 45 real integration tests counted by their integration module paths: admin 1, auth 22, invites 7, operations 15 (`/private/tmp/kamori-readiness-20261015/db-tests-full-server.log:162`). Successful full-suite/prerequisite/Clippy logs contain zero warning/error lines; the Clippy log records completion and the formatting log is empty as reported (`/private/tmp/kamori-readiness-20261015/db-tests-clippy.log:287`, `/private/tmp/kamori-readiness-20261015/db-tests-fmt.log`).

### Assessment

**Task quality:** Approved.

**Reasoning:** The implementation closes silent DB-test success paths with a shared prerequisite, preserves existing test isolation and concurrency coverage, and supplies meaningful behavioral RED-to-GREEN evidence. No task-scoped correctness, scope or maintainability issue was found.
