# Contributing to DynamicLight

Browse the [backlog](BACKLOG.md) and open a GitHub issue before starting a large change. Each planned feature includes an outcome and acceptance criteria; backlog inclusion is not a release commitment.

Keep future features in Backlog and use In Progress only while implementing them. Close delivered implementation issues after linking any outstanding verification to an open Validation tracker. Preserve unchecked checks at handoff; builds and simulations do not count as physical or assistive-technology results. Record defects in the validation tracker or a linked bug, and retain platform limitations explicitly. Completed work has no active priority.

When reconciling GitHub, update `docs/backlog.json`, `docs/backlog-progress.json` and `docs/github-backlog.json`, then run `node scripts/render-backlog.cjs`. The publishing script only adds new work; it preserves existing issue bodies, labels, project membership and field values.

Start with the [developer guide](docs/development.md) for Windows, macOS, and Linux build prerequisites and commands. Keep USB protocol handling, capture processing, and the UI separate. Validate input at IPC boundaries. Keep USB and capture on the native worker and grant the webview only the capabilities it needs.

Document which operating systems and hardware you tested. Tests that change lighting or capture screen/audio must be explicit and separate from ordinary unit tests. Do not include screen captures, device serial numbers, local paths, credentials, or private logs in issue reports.

Pull requests should explain the user-visible outcome, reference the relevant issue, and list validation and outstanding platform limitations. Contributions are licensed under the repository's MIT license.
