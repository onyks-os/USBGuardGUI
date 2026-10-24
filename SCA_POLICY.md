# Software Composition Analysis (SCA) Policy

USBGuardGUI uses **Dependabot** to monitor vulnerabilities in its dependencies, plus
`make audit` for on-demand local scanning.

## Severity Thresholds

- **Critical / High**: must be resolved within **7 days** of notification.
- **Medium**: resolved within **30 days**.
- **Low**: resolved in the next release, or accepted with a documented justification.

## Remediation Process

1. Dependabot automatically opens a pull request with the update.
2. The pull request is merged once CI passes.
3. If the update is not possible (for example a breaking change), we evaluate a backported patch or
   isolation of the dependency, and document the accepted risk in
   [`docs/security-assessment.md`](docs/security-assessment.md).

## License Compliance

Dependencies must carry a license compatible with the project license (MIT). Any conflict
must be resolved before merging. Every direct dependency is recorded in
[`DEPENDENCIES.md`](DEPENDENCIES.md) with its license.

`make dependency-policy`, also part of `make verify` and CI, enforces the SPDX
allow-list in [`deny.toml`](deny.toml) on the complete root-manifest graph:
optional, build, development and target-specific dependencies are included.
Unknown licenses and expressions without an allowed choice fail. This check
does not cover system-library licenses or separately installed development tools.
The same gate rejects unapproved Git and alternate-registry sources.

Install the pinned checker with `cargo +stable install cargo-deny --version 0.20.2 --locked`.
The checker uses the stable development toolchain; it does not raise the
application's Rust 1.85 minimum. `make test-dependency-policy` verifies rejection
behavior against local disposable dependency graphs.

## Before Each Release

- Run `make audit` and verify there are no unresolved vulnerabilities.
- Run `make dependency-policy test-dependency-policy` and resolve policy failures.
- Review outstanding Dependabot alerts manually.
- If unresolved Critical or High vulnerabilities remain, **the release is blocked** until remediation.
