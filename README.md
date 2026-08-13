<p align="center">
  <img src="src-tauri/icons/icon.png" alt="Soundboard icon" width="128" height="128">
</p>

<h1 align="center">Soundboard</h1>

<p align="center">
  A fast, local-first desktop soundboard with overlapping playback and global keyboard shortcuts.
</p>

## Download

[Download the latest release](../../releases/latest) from GitHub Releases:

- **macOS — Apple Silicon:** choose the `aarch64.dmg` asset.
- **macOS — Intel:** choose the `x86_64.dmg` asset.
- **Windows — 64-bit:** choose the `x86_64-setup.exe` asset.

The current installers are not signed with commercial Apple or Windows certificates. macOS Gatekeeper or Windows SmartScreen may therefore show a warning on first launch. See [Release signing](docs/RELEASING.md#release-signing) for details.

## Features

- Import MP3, WAV, OGG/Vorbis, and FLAC audio.
- Trigger sounds with a click, keyboard activation, or a system-wide shortcut.
- Start repeated, overlapping playback without cutting off earlier sounds.
- Arrange sounds in a configurable grid from 1 × 1 to 12 × 12.
- Resize safely: sounds are compacted into empty cells and are never silently deleted.
- Replace or delete sounds and add, change, or remove shortcuts from each cell's context menu.
- Keep working offline with no account, analytics, telemetry, or cloud service.
- Preserve imported audio in app-managed storage, independent of the original file.

## Using Soundboard

1. Select an empty cell and choose an audio file.
2. Select a filled cell to play it.
3. Right-click a filled cell to change its sound, delete it, or manage its shortcut.
4. Select the grid-size control in the header to change rows and columns.

Global shortcuts remain active while Soundboard is running, even when its window is unfocused or minimized. Shortcut recording temporarily suspends existing shortcuts so assigning a key never plays another sound.

## Development

### Prerequisites

- [Node.js 24](https://nodejs.org/) or another Node.js release supported by Vite 7
- [Rust 1.97.1](https://www.rust-lang.org/tools/install) (the repository toolchain file selects it)
- The [Tauri 2 platform prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system

### Run locally

```bash
npm ci
npm run tauri dev
```

Browser-only UI development uses an in-memory mock backend:

```bash
npm run dev
```

### Validate

```bash
npm run validate
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

### Build an installer locally

Run the command for your current operating system:

```bash
# macOS
npm run tauri -- build --bundles dmg

# Windows
npm run tauri -- build --bundles nsis
```

Cross-platform release installers are built automatically by GitHub Actions. The complete process is documented in [docs/RELEASING.md](docs/RELEASING.md).

## Architecture

- **UI:** Svelte 5, TypeScript, and Vite
- **Desktop runtime:** Tauri 2
- **Native core:** Rust
- **Audio:** Kira with CPAL
- **Persistence:** crash-safe local JSON plus app-managed audio copies

The frontend/backend contracts and product decisions are captured in [soundboard-frontend-spec.md](soundboard-frontend-spec.md) and [soundboard-backend-spec.md](soundboard-backend-spec.md).

## Privacy

Soundboard stores its configuration and imported audio locally in the operating system's application-data directory. It does not contain network calls, user accounts, analytics, or telemetry.
