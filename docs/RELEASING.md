# Releasing Soundboard

GitHub Actions builds native installers on the operating system that will run them:

| Target | GitHub runner | Release asset |
| --- | --- | --- |
| macOS Apple Silicon | `macos-latest` | `.dmg` (`aarch64`) |
| macOS Intel | `macos-latest` | `.dmg` (`x86_64`) |
| Windows 64-bit | `windows-latest` | NSIS `.exe` (`x86_64`) |

## Create a release

1. Update the version in all four files:
   - `package.json`
   - `package-lock.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
2. Run the validation commands from the README.
3. Commit the version change.
4. Push a matching semantic-version tag:

```bash
git tag v0.1.0
git push origin main v0.1.0
```

The `Release installers` workflow validates the project, builds both macOS architectures and Windows x64, creates a public GitHub Release, and attaches the installers. A failed platform build does not cancel the other matrix jobs.

## Release signing

The default workflow uses an ad-hoc macOS identity (`APPLE_SIGNING_IDENTITY=-`). This avoids a common “app is damaged” result for downloaded Apple Silicon builds, but it does **not** provide Apple notarization. Windows installers are also unsigned by default. Users may see Gatekeeper or SmartScreen warnings.

For a broadly distributed production release, configure real signing credentials before tagging:

- [Tauri macOS code signing and notarization](https://v2.tauri.app/distribute/sign/macos/)
- [Tauri Windows code signing](https://v2.tauri.app/distribute/sign/windows/)

Store certificates and passwords only as GitHub Actions secrets. Never commit them to the repository.
