# Releasing Stora

## Continuous integration

Every pull request and push to `main` runs the Windows CI workflow. It installs
the locked Node dependencies, type-checks and tests the frontend, runs all Rust
tests, and checks a release Rust build.

## One-time GitHub setup

In **Settings → Secrets and variables → Actions**, add:

- `TAURI_SIGNING_PRIVATE_KEY`: the complete contents of your local Tauri
  updater private-key file.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the password used when the key was
  generated.

Never commit, upload, or paste either secret into an issue, pull request, or
chat. Back up the private key and password securely: users installed with its
matching public key can only accept future updates signed by the same key.

## Publishing a release

1. Update the app version in `apps/desktop/src-tauri/tauri.conf.json` and
   `apps/desktop/package.json` together.
2. Commit and push the version change to `main`.
3. Create and push the matching version tag, for example:

   ```powershell
   git tag v0.1.1
   git push origin v0.1.1
   ```

4. GitHub Actions creates a **draft** GitHub Release with the Windows MSI and
   NSIS installers. Review its title, notes, and files, then publish the draft
   on GitHub.

The in-app updater is enabled only after its public key is added to the Tauri
configuration. The release workflow is already prepared to sign those updater
artifacts once that step is complete.
