# Security Policy

## Reporting a Vulnerability

We take the security of USBGuardGUI seriously. If you discover a security vulnerability in this
project, please **do not open a public issue**.

### How to report (preferred method)

Use GitHub's **private vulnerability reporting**:

1. Go to [https://github.com/onyks-os/USBGuardGUI/security/advisories](https://github.com/onyks-os/USBGuardGUI/security/advisories)
2. Click **"Report a vulnerability"**
3. Fill out the form with as much detail as possible:
   - Description of the issue
   - Steps to reproduce
   - Affected versions
   - Potential impact

### Alternative contact

Private vulnerability reporting on GitHub is the **only** channel the project monitors. The
maintainer's commit address is a GitHub `noreply` address and does not receive mail, so there is no
email fallback: a report sent there will not reach anyone.

If you are unable to use the GitHub form and the issue cannot wait, open a public issue containing
*nothing but* a request for a private channel — no reproduction steps, no affected versions, no
description of the flaw — and a private advisory will be opened for you.

## What to expect

- You will receive an acknowledgment within **48 hours**.
- We will investigate and keep you informed of progress at least every **7 days**.
- Once a fix is ready, we will credit you in the release notes (unless you prefer to remain anonymous).

## Public Disclosure

When a vulnerability is confirmed and fixed, USBGuardGUI publishes a public advisory containing:

- Affected versions
- Description of the issue
- Mitigation or upgrade instructions

The advisory is published on:

- **GitHub Security Advisories** — `https://github.com/onyks-os/USBGuardGUI/security/advisories`
- **Release notes** of the fixed version

We do not currently assign CVEs, but may do so in the future.

## Scope

This policy applies to the USBGuardGUI core modules and its public interfaces.

The following are considered **critical scope targets**:

USBGuardGUI is an unprivileged client that asks a root daemon to change USB authorization policy.
Its security value therefore rests on two properties: it acquires no privilege of its own, and every
privileged effect it causes is one the user actually asked for. An issue is **critical** when it
breaks one of those.

1. **Causing an authorization change the user did not request.** Any path by which a device is
   allowed, blocked, or rejected, or a rule is appended or removed, without the user having
   initiated and confirmed that specific action — including through a crafted device name or
   attribute arriving from the daemon, a notification quick-action, or a D-Bus reply.
2. **Acting on the wrong rule.** The rule-ID volatility race (`docs/architecture.md` §6.4) is
   narrowed but not closed client-side. A report that turns it into a *reliable* way to remove or
   modify a rule other than the selected one is critical, not informational.
3. **Breaking the unprivileged-client invariant.** Any read or write of a path under `/etc`,
   `/var`, or `/sys`; any setuid, capability, or helper-daemon acquisition; any privileged
   operation performed by the program rather than requested of the daemon.
4. **Rule-text injection.** Input that escapes the quoting of the rule generator
   (`docs/architecture.md` §7) so that an attacker-influenced device attribute becomes a rule
   fragment with its own semantics — a device name that closes a quote and appends a condition.
5. **Memory-safety or denial-of-service in the parser.** A panic, hang, or unbounded allocation
   reachable from daemon-supplied text. The parser is fuzzed precisely because every byte it sees
   originates outside this program's control.
6. **Leaking device identity.** Serial numbers, device hashes, or device names reaching a log at
   default level, a crash report, or a notification body where the policy says they must not. A
   default-level log attached to a bug report must not be an inventory of the user's hardware.

Out of scope:

- Vulnerabilities in third-party dependencies already tracked upstream (report them upstream, then
  open an issue here referencing the advisory).
- Findings that require an already-compromised host or physical access.
- **Weaknesses in the USBGuard daemon, its D-Bus bridge, its bus policy, or its IPC access-control
  implementation.** Those belong to [upstream USBGuard](https://github.com/USBGuard/usbguard). This
  project is a client of that contract; a report that the daemon itself authorizes too much is an
  upstream issue. A report that *this program* misreads or misrepresents the daemon's answer is not.
- **A permissive local configuration.** An administrator who installs a Polkit rule granting an
  unprivileged user the right to change USB policy has made that decision deliberately; the example
  rule under `packaging/` is shipped inert and never installed. That it *can* be enabled is the
  design, not a finding.

For the full STRIDE threat model, trust boundaries, risk severity ratings, and security controls
inventory, see **[`docs/security-assessment.md`](docs/security-assessment.md)**.

## Informal Bug Bounty & Hall of Fame

There is no financial budget for monetary rewards, but the project recognizes researchers who help
make USBGuardGUI safer. For valid, in-scope reports that are confirmed and resolved:

- **Permanent inclusion** in [`HALL_OF_FAME.md`](HALL_OF_FAME.md), with a link to the researcher's
  GitHub profile or personal website.
- **Honorable mention** in the GitHub Release Notes of the fixed version.

## Release support policy

| Version   | Support status      | End of life                    |
| --------- | ------------------- | ------------------------------ |
| 0.1.0   | ✅ Security fixes   | When the next minor is released |
| < 0.1.0 | ❌ Unsupported      |                                |

- Security fixes are provided only for the latest minor version.
- If you need long-term support, contact the maintainers.

## Security Hardening of the Project Itself

- All dependencies are monitored by Dependabot; see [`SCA_POLICY.md`](SCA_POLICY.md).
- Static analysis runs on every pull request; see [`SAST_POLICY.md`](SAST_POLICY.md).
- Secrets are never committed; see [`SECRETS_POLICY.md`](SECRETS_POLICY.md).
- Release artifacts are signed and reproducible; see [`docs/verification.md`](docs/verification.md).

## Acknowledgments

We thank the community for responsibly disclosing security issues. Contributors who report valid
vulnerabilities are publicly acknowledged unless they request otherwise.
