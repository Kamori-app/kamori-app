- **I1 — the promised fixed local endpoint is not actually selected.** — **ADDRESSED.** `tests/release-readiness/restore/run.py:22` fixes the absolute Podman backend; `:23` rejects unsupported `RESTORE_ENGINE`; `:25` fixes the reserved Unix socket; `:26`–`:28` strip all seven Podman/Docker endpoint selectors; `:59` passes `--url` on every engine invocation. `test_controls.py:36` exercises four conflicting local-selector combinations against the real daemon, checks selector removal and logged binding; `:57` checks override rejection. This removes reliance on the Docker wrapper and its ignored `DOCKER_HOST` setting.
- **M1 — partial container creation is not included in cleanup accounting.** — **ADDRESSED.** `tests/release-readiness/restore/run.py:103` records the exact attempted name before dispatch at `:104`. Cleanup requires successful inventory at `:113`, skips absence at `:115`, verifies both labels at `:116`, and removes at `:120`. Final inventory at `:131` includes attempted names. The original failed command still raises at `:54`. `test_controls.py:109`, `:127` and `:136` cover actual post-allocation failure, pre-allocation failure and ownership refusal respectively.

### New Breakage in the Fix Diff

- None found.

### Out-of-Scope Observations

- None.

### Checks

- Read the supplied `review-0b1cc7f..25f0bec.diff` once, original full review, brief, global constraints and appended fix report; inspected the affected runner control flow. No git rederivation, source/index/branch changes, subagents, suite/probe reruns or shared-resource operations.
- Inspected existing `restore-controls-red.log`: endpoint probes fail on nonexistent local Podman selectors, unsupported override fails its rejection assertion, and failed-start cleanup fails with the original untracked-allocation implementation. These are recorded executions, not independently rerun historical results.
- Inspected existing `restore-controls-final.log`: all five named controls report `ok`, `Ran 5 tests in 1.301s`, `OK`, consistent with the report's exit-zero claim. The tests assert actual start exit127 and Status=created, early failure exit125, original exception propagation, ownership refusal and independent absence checks.
- Inspected all three final mutation-control `cleanup.json` files and `final-owned-inventory.json` under `/private/tmp/kamori-readiness-20261015/restore/controls-xmbfp44h`: errors and container/volume/network inventories are empty. No new executable check was needed because the existing focused runs answer the named risks.

### Verdict

- **Fix round:** All findings addressed, no new Critical/Important breakage. Scope: `0b1cc7fd6960e01241f8ab7ea5ed66fd86238b5d..25f0bec9be862c496a4706eb83107e94b930fc08` only.
