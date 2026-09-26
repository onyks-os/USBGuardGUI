<!--
Copyright (c) 2026 onyks-os
SPDX-License-Identifier: MIT
-->

# Releasing USBGuardGUI

A release is a tag. Pushing `v<version>` runs
[`.github/workflows/release.yml`](../.github/workflows/release.yml), which builds every package,
signs everything, publishes the GitHub Release, and publishes the crate to crates.io.

| Job | Produces |
| :-- | :------- |
| `version` | Fails unless the tag's version equals `version` in `Cargo.toml`. |
| `native` | `.deb`, `.rpm`, the bare binary, and the `.crate` — built on Ubuntu 24.04, whose glibc is older than Fedora's, so one binary serves both. |
| `arch` | `.pkg.tar.zst`, built by `makepkg` in an Arch container from the tag's own tree. |
| `flatpak` | `.flatpak`, a single-file bundle for any distribution. |
| `release` | SBOM, `SHA256SUMS`, a Sigstore signature for every file, and the GitHub Release. |
| `crates-io` | `cargo publish`. Never for a pre-release. |

## Before tagging

1. `make verify` is green on `main`, and so is CI.
2. `version` in `Cargo.toml` is the version to release.
3. In `CHANGELOG.md`, the `[Unreleased]` section becomes `[<version>] - <date>`.
4. In `data/io.github.onyks_os.UsbguardGui.metainfo.xml.in`, a `<release>` entry for the version, then
   `make i18n` to regenerate the metainfo.

## Rehearse with a pre-release

A tag with a suffix is a **pre-release**: the whole pipeline runs, the GitHub Release is marked
*Pre-release* (never *Latest*, so the README's download links do not move), and nothing is
published to crates.io.

```bash
git tag -a v0.1.0-rc.1 -m "v0.1.0-rc.1" && git push origin v0.1.0-rc.1
```

Afterwards, delete it: `gh release delete v0.1.0-rc.1 --yes --cleanup-tag`.

## Release

```bash
git tag -a v0.1.0 -m "v0.1.0" && git push origin v0.1.0
```

## Verify what was published

```bash
gh release download v0.1.0
sha256sum --check SHA256SUMS
python -m pip install sigstore
sigstore verify identity \
  --cert-identity "https://github.com/onyks-os/USBGuardGUI/.github/workflows/release.yml@refs/tags/v0.1.0" \
  --cert-oidc-issuer "https://token.actions.githubusercontent.com" \
  --bundle usbguard-gui_0.1.0-1_amd64.deb.sigstore.json usbguard-gui_0.1.0-1_amd64.deb
```

## crates.io credentials

The `crates-io` job runs in the GitHub environment **`crates-io`**, which only tags matching `v*`
can use: a workflow started from a branch or a pull request never sees its secrets.

### First publish: an API token

crates.io does not allow a crate's very first publish through Trusted Publishing, so the first one
needs a token.

1. On crates.io: **Account Settings → API Tokens → New Token**. Scopes **publish-new** and
   **publish-update**, crate pattern `usbguard-gui`, and a short expiry — it is only needed once.
2. Store it in the environment. The command asks for the value without echoing it:

   ```bash
   gh secret set CARGO_REGISTRY_TOKEN --env crates-io --repo onyks-os/USBGuardGUI
   ```

   Or on GitHub: **Settings → Environments → crates-io → Add environment secret**.

Never paste the token into an issue, a commit, or a chat.

### Afterwards: Trusted Publishing, no stored token

Once the crate exists on crates.io:

1. On crates.io: the crate's **Settings → Trusted Publishing → Add → GitHub**, with repository
   owner `onyks-os`, repository `USBGuardGUI`, workflow `release.yml`, environment `crates-io`.
2. Tell the workflow to use it:

   ```bash
   gh variable set CRATES_IO_TRUSTED_PUBLISHING --body true --repo onyks-os/USBGuardGUI
   ```

3. Remove the token: `gh secret delete CARGO_REGISTRY_TOKEN --env crates-io --repo onyks-os/USBGuardGUI`,
   and revoke it on crates.io.

From then on each release gets a token valid for 30 minutes, minted for that run of that workflow
and revoked when the job ends.

## If a job fails

Nothing is published before every package has been built: the `release` job needs all of them. If
`crates-io` fails after the GitHub Release exists, fix the cause and re-run only that job from the
workflow run page. A failed release otherwise: delete the tag
(`gh release delete v0.1.0 --yes --cleanup-tag`, or `git push origin --delete v0.1.0`), fix, tag again.
