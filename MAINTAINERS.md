# Maintainers

This document lists the maintainers of USBGuard GUI (USBGuardGUI) and defines the processes
governing maintainership responsibilities, access, and lifecycle.

---

## Project Lead

| Handle       | GitHub                                             | Role                     | Since        |
| :----------- | :------------------------------------------------- | :----------------------- | :----------- |
| `onyks` | [@onyks-os](https://github.com/onyks-os) | Creator & Lead Developer | 2026 |

## Core Contributors

*We are actively looking for contributors who want to take on a maintainer role. See
[CONTRIBUTING.md](CONTRIBUTING.md) for how to get involved.*

## Emeritus Maintainers

*Maintainers who have stepped down but made significant past contributions are listed here with our
gratitude.*

---

## Project Roles

The table maps each operational role to its current holder. One person may hold multiple roles, as is
the case for a solo-maintained project.

| Role                 | Current Holder | Responsibilities                                                                                     |
| :------------------- | :------------- | :--------------------------------------------------------------------------------------------------- |
| **Project Lead**     | `onyks`  | Final decision-maker on architecture, roadmap, and breaking changes.                                 |
| **Code Reviewer**    | `onyks`  | Reviews and approves pull requests; enforces coding standards and architectural principles.          |
| **Release Manager**  | `onyks`  | Owns the release pipeline: version bump, changelog, artifact build, publication, and signing.        |
| **Security Officer** | `onyks`  | Triages private disclosures, coordinates fixes, publishes advisories, keeps `SECURITY.md` current.   |
| **CI/CD Maintainer** | `onyks`  | Maintains GitHub Actions workflows, manages repository secrets, keeps the pipeline healthy.          |

> As the project gains contributors, roles will be distributed and this table updated accordingly.

---

## Access to Sensitive Resources

| Resource                   | Access Holder | Notes                                                    |
| :------------------------- | :------------ | :------------------------------------------------------- |
| GitHub Repository (Admin)  | `onyks` | Full admin access                                        |
| Package Registry           | `onyks` | Owner of the published package                           |
| GitHub Actions Secrets     | `onyks` | Manages CI/CD credentials                                |
| Release Signing            | `onyks` | Signs release artifacts (Sigstore keyless / GPG)         |
| Security Reporting Inbox   | `onyks` | `github.tgwp7@aleeas.com` — see [SECURITY.md](SECURITY.md)        |

---

## Responsibilities

Maintainers are expected to:

- **Review and merge pull requests** in a timely manner (target: within 7 days).
- **Triage issues**: label, respond to, and close stale issues.
- **Enforce the security policies** defined in [SECURITY.md](SECURITY.md).
- **Ensure CI passes** (`make verify`) before merging any change.
- **Manage releases**: version bump, changelog, artifact build, and publication.
- **Respond to security disclosures** within 48 hours.

---

## Merge Policy

- All changes to `main` go through a pull request. Direct pushes are reserved for critical hotfixes.
- At least **one maintainer approval** is required before merging.
- All CI checks must pass.
- Pull requests touching security-critical paths require explicit sign-off from the Project Lead.

  The security-critical paths are:

  | Path | Why |
  | :--- | :-- |
  | `src/rules/` | Parses untrusted daemon-supplied text and generates rule syntax. Rule injection and parser panics both originate here. |
  | `src/dbus/` | The trust boundary with the daemon — proxies, the request path, rule-identity handling, and denial attribution. |
  | `packaging/` | Contains the example Polkit rule. A change that caused it to be *installed* rather than shipped inert would silently grant privilege. |
  | `Cargo.toml`, `Cargo.lock`, `clippy.toml` | A new dependency, a widened feature set, or a relaxed lint is a change to the attack surface. `clippy.toml` carries the `disallowed-methods` lint that enforces the no-panic rule. |
  | `.github/workflows/` | Release signing, SBOM generation, and every token permission. |
  | `SECURITY.md`, `docs/security-assessment.md` | The stated threat model. |

  These are the same paths listed in [`.github/CODEOWNERS`](.github/CODEOWNERS); the two must stay
  in step.

---

## Becoming a Maintainer

Maintainership is granted based on sustained, high-quality contributions:

1. Contribute multiple non-trivial pull requests that are reviewed and merged.
2. Demonstrate understanding of the architecture and security model.
3. Be nominated by the Project Lead or an existing Core Contributor.
4. Accept the [Code of Conduct](CODE_OF_CONDUCT.md) and the responsibilities described here.

---

## Offboarding a Maintainer

When a maintainer becomes inactive or steps down:

1. They are moved to the **Emeritus** section of this file.
2. All access (GitHub admin, registry, secrets) is revoked promptly.
3. Any release signing keys or credentials they held are rotated.
4. A note is added to the changelog if they made significant contributions.

---

## Bus Factor

This project currently has a bus factor of **1**. All credentials required to continue the project
are documented above. In the event the Project Lead becomes unreachable for more than 90 days,
maintainership may be claimed by a Core Contributor by opening a public issue and, absent objection
within 30 days, requesting a repository transfer through GitHub Support.
