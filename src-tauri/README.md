# Soundboard native core

This directory contains the complete Tauri v2 backend. The frontend talks to it only through the commands registered in `src/lib.rs`; file access, audio decoding, persistence, and global-shortcut registration stay in Rust.

## Development checks

Run from the repository root:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
cargo build --manifest-path src-tauri/Cargo.toml --release
```

After the frontend has been built into `dist/`, `cargo run --manifest-path src-tauri/Cargo.toml` launches the desktop application without requiring the JavaScript Tauri CLI.

## Runtime data

The backend resolves Tauri's application-data directory for `dev.homyc.soundboard` and owns `state.json`, crash-recovery candidates, corrupt-state backups, and UUID-named managed audio copies below that directory. Original import paths are never persisted or used after import.

Mutations are coordinated as transactions: native selection happens outside the mutation gate, slow work happens without the state lock, persistence completes before in-memory state is published, and pre-commit cache/files/hotkeys are rolled back on failure. Kira and decoded audio live on one worker thread; the global-shortcut callback only performs a lookup and a non-blocking enqueue.

## Manual release matrix

The automated tests use fake audio, hotkey, repository, and picker adapters, so they never play audio or register shortcuts. Before distributing a release, perform the specification's manual matrix on Windows, macOS, and Linux: all four formats, rapid overlap, unfocused/minimized shortcuts, restart recovery, replace/delete lifecycle, blocked shrink details, missing device/file recovery, and shortcut release at process exit.
