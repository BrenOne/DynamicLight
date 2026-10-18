# Contributing to DynamicLight

Browse the [backlog](BACKLOG.md) and open a GitHub issue before starting a large change. Each planned feature includes an outcome and acceptance criteria; backlog inclusion is not a release commitment.

Use Node.js 24 and `pnpm install --frozen-lockfile`, then `pnpm test`. Run `pnpm start` to try the app. Keep USB protocol handling, capture processing, and the UI separate. Validate input at IPC boundaries and keep the renderer sandboxed.

Document which operating systems and hardware you tested. Tests that change lighting or capture screen/audio must be explicit and separate from ordinary unit tests. Do not include screen captures, device serial numbers, local paths, credentials, or private logs in issue reports.

Pull requests should explain the user-visible outcome, reference the relevant issue, and list validation and outstanding platform limitations. Contributions are licensed under the repository's MIT license.
