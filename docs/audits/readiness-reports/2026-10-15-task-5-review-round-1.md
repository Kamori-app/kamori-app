- **Frontend default UI tests lack pinned Chromium/Linux provisioning** — **ADDRESSED**. `.github/workflows/ci.yml:164–166` adds the existing acceptance workspace's `bun run --cwd tests/acceptance playwright install --with-deps chromium` command after frozen dependency installation and before frontend tests. The Ubuntu job retains Bun 1.3.14 and the admin/desktop Turbo filters (`.github/workflows/ci.yml:140–172`). This provisions the same runner that executes the gates.
- **External UI-only edits can skip the behavioral frontend gate** — **ADDRESSED**. `scripts/ci/detect-affected.sh:73–76` selects frontend for the shared UI configuration, host helper, UI specs, bundler and fixture entries, while retaining the existing server selection. `scripts/ci/detect-affected.test.sh:89–111` exercises all nine actual inputs independently against the full seven-area expected output, plus two unrelated acceptance negative controls. `.github/workflows/ci.yml:143` selects the frontend job from that output; its test filters include both owning packages.

### New Breakage in the Fix Diff

- None. The three-file fix adds provisioning and narrow affected-path selection without changing application behavior, dependencies, package commands or existing detector classifications.

### Out-of-Scope Observations

- None newly observed. The previous review's disclosed adapter empty-chunk notice remains outside this fix.

### Checks and Evidence

- Read the supplied `review-a313793..677ec05.diff` once: three files, 34 insertions; review limited to base `a313793d88f26d8a19530119bfaef11c5c14edc3` through head `677ec051e21e5c3f9d461590630ab11e8d3eca60`, the two findings, and their evidence. No git re-derivation, source/index/ref changes, subagents, detector reruns or UI reruns.
- Read the brief, previous review and appended fix report. Inspected only the workflow/detector/test context needed to connect the changed patterns and assertions to actual frontend job selection.
- Inspected saved `ui-ci-detector-red.log`: the new configuration-input assertion fails specifically because frontend was false. Inspected saved GREEN log (silent success) and trace: 18 executed `assert_case` calls plus the completed explicit `--all` equality check support the reported 19 scenarios (nine new positives, two new negatives, eight retained).
- Inspected saved `ui-ci-validate.mjs` and `ui-ci-workflow-validation.log`: the validator parses the workflow, checks Ubuntu/frontend conditions, pinned Bun and dependency→browser→test ordering, exact existing install command and admin/desktop filters, and syntax-checks all four frontend run scripts. Output records Playwright 1.62.1 and a successful install dry-run. The reported detector syntax and diff checks are consistent with the supplied small diff; no tests were repeated.
- Linux dependency installation, browser download and actual GitHub Actions execution were not performed by this review or established by the macOS dry-run. The implementer's report explicitly preserves that boundary and makes no fresh Linux runner success claim. Earlier seven UI cases, four existing admin units and package checks/builds remain prior evidence, unchanged by this fix.

### Verdict

- **Fix round:** **All findings addressed, no new Critical/Important breakage.** No open findings.
