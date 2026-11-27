# AI Policy

## How this project is made

USBGuardGUI is written entirely by AI. The code, tests, packaging, CI and documentation
are produced by an AI coding assistant (Anthropic's Claude, through Claude Code); commits
it writes carry a `Co-Authored-By` trailer. The human maintainer does not write the code.
They decide what is built, set the scope and priorities, and approve every merge and
release.

Contributions from other people keep their authorship. The Brazilian Portuguese catalogue,
for example, was written by its contributor.

## What we rely on instead

AI-written code can be wrong in ways that read as plausible, so the project does not ask
to be trusted on that basis. It aims to be reliable through verification, and every change
goes through the same gates whoever, or whatever, wrote it:

- **Every pull request:** formatting and Clippy with warnings as errors; tests on stable,
  beta and the minimum supported Rust version; a whole-application line-coverage gate
  (>=95%, GTK included); fuzzing of the rule parser; a dynamic-analysis gate; CodeQL;
  dependency advisories, licenses and sources; a strict documentation build; DCO sign-off.
- **Every package format:** two independent offline builds must produce byte-identical
  binaries and RPM, DEB, Arch and Flatpak packages. See
  [reproducing the build](docs/verification.md#5-reproducing-the-build).
- **Every release:** the Flatpak bundle is installed and started in its pinned runtime;
  every artifact gets a checksum, an SBOM entry and a Sigstore signature bound to the
  release workflow; a release candidate is published and checked before the stable tag.
- **Behavior on real systems:** for changes that affect it, clean installations in disposable
  virtual machines (Fedora, Debian, Ubuntu, Arch) exercise the real daemon, Polkit and PAM
  with synthetic USB devices. Graphical tests run on private displays or in those machines,
  never on a personal desktop.

What each release was checked against, and what was not repeated, is recorded in
[docs/verification.md](docs/verification.md).

## What this does not claim

Passing these checks is evidence, not proof. Tests only cover what someone thought to test,
and coverage measures lines executed, not correctness. The pipeline has caught AI mistakes
before they shipped (in 0.2.1, a metadata tag that differed between distributions and a
stale Flatpak runtime pin), which is the reason it exists, not a guarantee that nothing
slips through. Report problems as described in [SECURITY.md](SECURITY.md) or
[SUPPORT.md](SUPPORT.md).

## Contributions that use AI

They are welcome on the same terms as any other: the same gates apply, and the
contributor signs off under the DCO and answers for the change. Please say in the pull
request whether, and how, AI was used. Translations prepared with AI are marked as such
in [docs/translating.md](docs/translating.md) until a fluent speaker has reviewed them.
