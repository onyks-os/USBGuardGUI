# Release Verification

Every USBGuardGUI release is published with checksums, a Sigstore signature, and a CycloneDX
SBOM. This guide shows how to verify an artifact before installing it.

## 1. Verify the Checksums

Download the artifact and `SHA256SUMS` from the
[Releases page](https://github.com/onyks-os/USBGuardGUI/releases), then:

```bash
sha256sum --check --ignore-missing SHA256SUMS
```

Expected output: `<artifact>: OK`.

## 2. Verify the Sigstore Signature

Release artifacts are signed keylessly through GitHub Actions OIDC — there is no long-lived private
key to steal. Install [`sigstore`](https://pypi.org/project/sigstore/) and verify:

```bash
python -m pip install sigstore

sigstore verify identity \
  --cert-identity "https://github.com/onyks-os/USBGuardGUI/.github/workflows/release.yml@refs/tags/v0.1.0" \
  --cert-oidc-issuer "https://token.actions.githubusercontent.com" \
  <artifact>
```

## 3. Verifying Signer Identity

The `--cert-identity` value must exactly match the release workflow path and the tag being verified.
A signature that verifies against a *different* identity is not a valid USBGuardGUI release,
even if the cryptography checks out.

## 4. Inspect the SBOM

```bash
jq '.components[] | {name, version, licenses}' sbom.json
```

Compare the component list against [`DEPENDENCIES.md`](../DEPENDENCIES.md). Any component present in
the SBOM but absent from that file should be reported as an issue.

## 5. Reproducing the Build

```bash
git clone --branch v0.1.0 https://github.com/onyks-os/USBGuardGUI.git
cd USBGuardGUI
make build
sha256sum dist/*
```

**The build is not currently bit-for-bit reproducible, and no release claims that it is.**

Saying so plainly is the point of this section: a reproducibility claim that has never been tested
is worse than no claim, because it invites verification that will fail for uninteresting reasons and
teach the verifier to ignore the result.

What is known to vary between two builds of the same tag:

| Source of variation | Effect | Normalization |
| :------------------ | :----- | :------------ |
| Absolute paths of the build directory | Embedded in panic messages and debug info | `RUSTFLAGS="--remap-path-prefix=$PWD=."`, plus the same for `$CARGO_HOME` |
| Build timestamp in the package metadata | Differs per build | `SOURCE_DATE_EPOCH`, set from the tag's commit date |
| Toolchain version | Different codegen | `rust-toolchain.toml` pins the exact toolchain |
| Host C toolchain and system GTK headers | The `-sys` crates link against whatever the host provides | Only a container image with a pinned base is comparable; two different distributions will not match |

What *is* guaranteed today, and is what the verification steps above actually check:

- The `SHA256SUMS` file covers every published artifact.
- Each artifact is signed with Sigstore keyless signing, and the signature identifies the workflow,
  the repository, and the commit that produced it.
- A CycloneDX SBOM lists every component that went into the build.

Together those establish *provenance* — this artifact came from this commit, built by this workflow
— which is a different and weaker property than reproducibility, and the one this project can
currently stand behind. Making the build reproducible is tracked as future work; when it lands, this
section will state the exact container image and environment required to reproduce a release, and
the claim will be tested in CI rather than asserted here.
