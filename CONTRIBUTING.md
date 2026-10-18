# Contributing to DynamicLight

Browse the [backlog](BACKLOG.md) and open a GitHub issue before starting a large change. Each planned feature includes an outcome and acceptance criteria; backlog inclusion is not a release commitment.

Start with the [developer guide](docs/development.md) for Windows, macOS, and Linux build prerequisites and commands. Keep USB protocol handling, capture processing, and the UI separate. Validate input at IPC boundaries. Keep USB and capture on the native worker and grant the webview only the capabilities it needs.

Document which operating systems and hardware you tested. Tests that change lighting or capture screen/audio must be explicit and separate from ordinary unit tests. Do not include screen captures, device serial numbers, local paths, credentials, or private logs in issue reports.

Pull requests should explain the user-visible outcome, reference the relevant issue, and list validation and outstanding platform limitations. Contributions are licensed under the repository's MIT license.
