# SignPath code signing setup

The release workflow (`.github/workflows/release.yml`) builds Snowy on GitHub Actions and, when
signing is configured, submits `snowy.exe` and the installer to SignPath. Without the secret it
still works and produces an unsigned draft release.

## One-time setup (maintainer)

1. Apply to the SignPath Foundation (signpath.org/terms) and create the organization.
2. Create a project with slug `snowy` and connect it to this GitHub repository as the trusted
   build system (origin verification).
3. Artifact configurations (slug -> contents). The uploaded GitHub artifact is delivered as a zip:
   - `exe`: root `<zip-file>` containing `<pe-file path="snowy.exe">` with `<authenticode-sign/>`.
   - `installer`: root `<zip-file>` containing `<pe-file path="Snowy-*-setup.exe">` with
     `<authenticode-sign/>` (the Inno Setup output is a normal PE file).
4. Signing policy with slug `release-signing`: enable **manual approval** and restrict the origin
   to this repository (optionally only `v*` tags).
5. Create a CI user in SignPath with an API token that may submit to the policy.
6. In the GitHub repository settings:
   - Secret `SIGNPATH_API_TOKEN` = the CI user token.
   - Variable `SIGNPATH_ORGANIZATION_ID` = the SignPath organization id.

## Releasing

1. Bump the version in `Cargo.toml`; optionally add `docs/release-notes/<version>.md`.
2. Push tag `v<version>` (must equal the Cargo.toml version or the workflow fails).
3. The workflow submits two signing requests (exe, then installer). Approve each one in the
   SignPath web UI; the workflow waits for completion.
4. A **draft** release `v<version>` with `Snowy-<version>-setup.exe`, its `.sha256` and the
   portable zip is created. Review and publish it manually.

Manual runs (`workflow_dispatch`) work too; the draft release is attached to the current commit.
