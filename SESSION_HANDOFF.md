# Development handoff — 2026-10-25

The requested zbus/ksni migration and clean-install verification are implemented
and locally verified. No translation expansion or AUR publication was undertaken.

- zbus 6.1 and Rust 1.89 replace the old dependency/compiler baseline. The fixed
  SNI/menu implementation runs directly on zbus, replacing ksni and the deprecated
  zvariant stack. Architecture, dependency inventory and ADR 0002 describe it.
- All four real GUI/PAM cases pass on Ubuntu 24.04.5, Debian 13.7, Fedora 44 and
  Arch rolling: no agent, cancelled authentication, correct-password runtime block
  with unchanged permanent rules, daemon restart/GUI reconnection.
- Tests also verify wire properties/actions, both tooltip signals, watcher restart,
  mutation error classification and installed artifact identity. Core per-file
  coverage stays above 90%; the dependency audit has no findings or notices.
- [Testing](docs/testing.md#zbus-6-and-real-clean-system-validation-2026-10-2425)
  and [verification](docs/verification.md#post-012-zbus-6-validation-2026-10-2425)
  contain exact scope, reproduction and committed receipts.

Checkpoint artifacts under ignored `target/zbus6-repro-final` are measured
modified-worktree builds retaining version 0.1.2, distinct from the published
0.1.2 release. The native binary hash in all guest receipts is
`e6f620ab6756d34849a15357b9967fe1b07541a0b509686f3d930484ff53cf82`.
The CI reproduction gate measures the final PR source independently.

Local test VM disks and evidence remain under `target/clean-install-*`, with
credentials private and never committed. Stop VMs after collecting evidence.
Use generic Debian images with USB support, start the packaged modern Polkit
helper socket when needed, and reload fixture policy after writing it.
Do not enable SUID or disable SELinux to make a test pass.

No new release/tag is part of this implementation handoff. Publication needs the
usual versioning and release gates. Signed-off-by with the configured Git identity
was explicitly approved for this work; avoid asking for that consent again.

Developer tools: `/home/onyks/.cargo/bin` and `target/tools/bin` (cargo-deny 0.20.2,
cargo-llvm-cov 0.9.1). Use `/bin/bash`, `login:false`. The host lacks /dev/kvm,
so VM execution uses TCG. Historical detailed pause instructions are preserved
in commit `ccfb96d`; the work described there has now been completed.
