# Desktop Soundboard — Rust/Tauri Backend Implementation Specification

Version: 1.0
Status: implementation-ready MVP
Companion document: `soundboard-frontend-spec.md`

## 1. Purpose and agent boundary

Build the complete native core for a small cross-platform soundboard using stable Rust and the latest stable Tauri v2 release. The backend owns file picking/import, app-owned storage, durable state, audio decoding/playback, overlapping voices, global shortcuts, validation, error reporting, and the exact IPC/event contract consumed by the frontend.

The backend agent owns only `src-tauri/**`, including `Cargo.toml`, `Cargo.lock`, `tauri.conf.json`, capabilities, icons/bundle configuration, Rust tests, and Rust-side documentation. Do not edit frontend presentation files under `src/**`.

## 2. Fixed product decisions

- Target Windows, macOS, and Linux desktop.
- A shortcut is global for as long as the process runs, including when the window is unfocused or minimized, except while shortcut capture intentionally suspends native registrations.
- Closing the main window exits; no tray or autostart in the MVP.
- Output uses the current default system audio device. Output-device selection and microphone/virtual-cable routing are out of scope.
- Copy every successful import into the app data directory. Never persist dependence on the user's source path.
- Guarantee MP3, WAV, OGG/Vorbis, and FLAC.
- Default grid is 4 × 4; rows and columns are independently configurable from 1 through 12.
- One cell holds zero or one sound.
- Every trigger starts a new playback instance. Existing instances continue unchanged.
- Internal duplicate shortcuts are prohibited. An unavailable OS-level shortcut is reported but not silently replaced.
- No network access, accounts, analytics, or telemetry.

## 3. Technical baseline

At specification time (2026-08-13), Tauri's official release index lists `tauri` 2.11.5 as the current stable core. Bootstrap with the latest mutually compatible stable Tauri v2 CLI/API/core packages available at implementation time, commit lockfiles, and do not use prereleases. Tauri's official global-shortcut plugin supports Windows, Linux, and macOS and exposes Rust registration/unregistration APIs.

Use:

- stable Rust pinned through `rust-toolchain.toml`;
- Tauri v2;
- `tauri-plugin-global-shortcut` on desktop targets;
- `tauri-plugin-dialog` for the native single-file picker;
- Kira with its CPAL backend and only the Symphonia format/codec features needed for MP3, WAV/PCM, OGG/Vorbis, and FLAC;
- Serde/Serde JSON, UUID v4, `thiserror`, and a small logging facade.

Kira is selected because `StaticSoundData` can be cloned for simultaneous playback without duplicating its decoded sample storage. Do not add SQLite, an ORM, a web server, frontend filesystem permissions, or a second audio engine.

Primary references:

- [Tauri ecosystem releases](https://v2.tauri.app/release/)
- [Tauri global shortcut plugin](https://v2.tauri.app/plugin/global-shortcut/)
- [Tauri dialog plugin](https://v2.tauri.app/plugin/dialog/)
- [Tauri application data paths](https://v2.tauri.app/reference/javascript/api/namespacepath/)
- [Kira crate and supported formats](https://docs.rs/kira/latest/kira/)

## 4. Architecture

Keep business logic independent of Tauri adapters so it can be unit-tested. Suggested modules:

```text
src-tauri/src/
  lib.rs
  main.rs
  commands.rs
  domain.rs
  dto.rs
  error.rs
  coordinator.rs
  persistence/
    mod.rs
    json_repository.rs
  audio/
    mod.rs
    kira_service.rs
  hotkeys/
    mod.rs
    tauri_service.rs
    normalize.rs
  import/
    mod.rs
  events.rs
```

Define narrow internal traits such as `StateRepository`, `AudioService`, `HotkeyService`, and `FilePicker` with fake implementations for tests. Tauri commands call a single coordinator; commands must not independently mutate JSON, hotkey registration, or the audio cache.

Runtime ownership:

- The coordinator owns the validated domain state behind a poison-tolerant mutex or an equivalent serialized command boundary.
- A coordinator-wide mutation gate serializes commit transactions. Do not hold it while a native picker is open; acquire it after selection and revalidate. While the gate is held, clone/read or publish domain state under a short state lock, but never hold that state lock during decoding, file I/O, hotkey API calls, or persistence.
- A dedicated audio worker thread owns Kira's `AudioManager` and the decoded `StaticSoundData` cache. Communicate with it using a bounded channel plus one-shot responses for load/unload/play operations.
- The Tauri global-shortcut callback performs only a pressed-state check, a short lookup, and a non-blocking enqueue to the audio worker. Never decode audio, copy files, or wait on disk in the shortcut callback.
- Do not hold the state lock across a native file dialog, audio decoding, slow filesystem I/O, or frontend event emission.

## 5. Domain model and persistence

### 5.1 App data layout

Resolve the Tauri app data directory for the configured bundle identifier and create:

```text
<app-data>/
  state.json
  state.next.json       # only during crash-safe writes
  state.previous.json   # recovery candidate
  audio/
    <sound-uuid>.<ext>
  backups/
    state.corrupt-<utc-timestamp>.json
```

Never write mutable data into bundled resources or the install directory.

### 5.2 Persisted JSON schema

Persist only occupied cells. JSON field names are camelCase.

```json
{
  "schemaVersion": 1,
  "grid": { "rows": 4, "columns": 4 },
  "assignments": [
    {
      "cellId": "r0c0",
      "sound": {
        "id": "uuid",
        "displayName": "Air horn",
        "originalFileName": "air-horn.mp3",
        "storedFileName": "uuid.mp3",
        "format": "mp3",
        "durationMs": 1530,
        "shortcut": {
          "modifiers": ["ALT"],
          "code": "KeyF"
        }
      }
    }
  ]
}
```

Do not persist absolute source paths, dynamic `shortcutStatus`, runtime warnings, or playback instances.

Validation invariants:

- `schemaVersion` is recognized or migrated before use;
- grid dimensions are both 1–12;
- `cellId` parses to a coordinate inside the grid;
- cell IDs, sound UUIDs, stored filenames, and normalized shortcuts are unique;
- `storedFileName` is a basename matching the sound UUID and a supported lowercase extension;
- display names are non-empty after trimming and capped at 120 Unicode scalar values;
- a shortcut has exactly one non-modifier code and a canonical, duplicate-free modifier list.

### 5.3 Crash-safe writes and recovery

Every successful mutation must be durable before its command returns. Implement a same-directory, crash-recoverable replacement:

1. serialize and validate the complete next state;
2. write `state.next.json`, flush it, and sync its file data;
3. preserve the current valid file as `state.previous.json` using platform-safe renames/replacement;
4. move the fully written next file into `state.json`;
5. sync the containing directory where supported;
6. remove stale recovery files only after the new current file is known valid.

On startup, validate `state.json`; if absent/invalid, try valid recovery candidates. Move an unrecoverable corrupt file to `backups/`, start with the default 4 × 4 state, and include a warning in the initial snapshot. A corrupt or missing individual audio file must not reset unrelated assignments.

Schema migrations are explicit functions (`vN -> vN+1`) and covered by fixtures. An unknown future version puts persistence into a non-mutating error state and surfaces `STATE_VERSION_UNSUPPORTED`; never overwrite it with a default file.

## 6. Import, replace, and delete transactions

### 6.1 File rules

- Native picker filters: `mp3`, `wav`, `ogg`, `flac`.
- Maximum source size: 50 MiB. Return `FILE_TOO_LARGE` before decoding.
- Treat extension only as an initial filter. Successful decoder probing is authoritative.
- Reject directories, symlinks that cannot be resolved to a regular file, empty files, and files that change identity/size while being copied.
- Derive the display name from the original filename stem, trim it, and fall back to `Untitled sound`.
- Generate a new sound UUID and a backend-controlled stored basename. Never concatenate an untrusted filename into an app-data path.

### 6.2 Import into an empty cell

`pick_and_import_sound` must:

1. validate the requested empty cell under the state lock, then release it;
2. open the native picker; return `null` on cancellation;
3. size-check and decode the selected file to validated `StaticSoundData` off the UI thread;
4. copy it to a temporary file under `audio/`, flush/sync, then rename to its UUID filename;
5. re-check that the cell is still empty;
6. load the decoded data into the audio cache, persist the new complete state, then publish the in-memory state;
7. roll back the cache entry and copied file if persistence fails;
8. return a fresh full snapshot.

Only one mutation per cell may commit at a time. A stale concurrent import returns `CELL_OCCUPIED` rather than overwriting.

### 6.3 Replace

Replacement follows the same validation but writes a new UUID-named file and decoded cache entry before changing state. It preserves the old shortcut exactly. Commit the new state atomically, then unload and best-effort delete the old managed file. If any pre-commit step fails, the old sound, shortcut, file, and cache entry remain usable. Orphan cleanup may run at the next startup but must delete only UUID files not referenced by a validated current or recovery state.

### 6.4 Delete

For deletion:

1. locate the occupied cell;
2. unregister its shortcut, if any;
3. persist the state without the assignment;
4. if persistence fails, re-register the old shortcut and leave runtime state unchanged;
5. on success publish state, unload decoded data, and best-effort remove the managed audio file.

Never delete a user source file.

## 7. Audio service

- Initialize one Kira `AudioManager` for the process using the default output device.
- Preload and decode all valid persisted sounds at startup into a cache keyed by sound UUID. Loading failures mark only that sound `playable: false` and create a warning; the cell remains replaceable/deletable.
- Configure capacity for at least 128 simultaneous sound instances. When capacity is exhausted, return/emit `PLAYBACK_LIMIT_REACHED`; never stop the oldest voice implicitly.
- Each play request clones the cached `StaticSoundData` and starts a fresh instance. It must not reuse a single handle in a way that restarts, toggles, or cuts off another instance.
- Retain or reap playback handles as required by the pinned Kira API, without unbounded handle growth after sounds finish.
- On a successful start, generate a UUID `instanceId` and emit `soundboard://playback-started`.
- Command-triggered failure returns a structured command error. Global-shortcut-triggered failure emits `soundboard://playback-failed` because there is no caller awaiting a command result.
- If no output device exists at startup, the app still loads and permits sound management. Mark sounds unplayable for that runtime and return `AUDIO_DEVICE_UNAVAILABLE` on play. A restart may retry device initialization.

Acceptance performance on a normal release build: with audio already cached and the device available, enqueue-to-start work must avoid disk I/O; 20 rapid triggers of the same clip create 20 overlapping starts without interruption or a panic.

## 8. Global shortcuts

Use the Rust API of `tauri-plugin-global-shortcut`, not JavaScript registration.

### 8.1 Shortcut representation

A shortcut contains a set of unsided modifiers and one physical `Code`:

```text
modifiers: CONTROL, ALT, SHIFT, META
code: KeyA..KeyZ, Digit0..Digit9, F1..F24, and explicitly mapped supported codes
```

- Canonical modifier order is CONTROL, ALT, SHIFT, META.
- Left/right modifier variants normalize to the same modifier.
- Shortcut identity is the modifier bitset plus physical code; display strings are not identity.
- Require at least one modifier for letters, digits, punctuation, navigation keys, Space, Enter, Backspace, and Tab. Function keys F1–F24 may be bare.
- Reject modifier-only chords, unknown codes, and OS-specific keys that the Tauri plugin cannot register consistently.
- Build the display string in Rust for the current platform and return it in DTOs. Internal conflict matching never compares display strings.

### 8.2 Registration lifecycle

At startup, register every valid persisted shortcut and build both mappings:

- normalized shortcut -> cell/sound;
- plugin shortcut ID -> cell/sound.

React only to `ShortcutState::Pressed`; ignore release events. A trigger performs a current mapping lookup, enqueues a fresh play request with trigger `globalShortcut`, and returns immediately.

If a persisted shortcut cannot register because the OS or another process owns it, keep it persisted, return `shortcutStatus: "unavailable"`, and add a cell-scoped warning. The Tauri/OS API cannot identify the external owning application, so never invent one.

`set_shortcut_capture_active({ active: true })` suppresses callback routing and temporarily unregisters every active native shortcut before returning, so an existing chord can reach the focused WebView recorder without playing its sound. `active: false` re-registers persisted shortcuts, rebuilds their runtime mappings, marks any newly unavailable registrations, and only then resumes callback routing. Both transitions are idempotent. Keys pressed while the application is inactive are not recorded by the frontend.

### 8.3 Assignment transaction and conflicts

`set_shortcut` must be race-safe:

1. normalize and validate the requested chord;
2. compare it with every other assignment under the state lock;
3. on an internal duplicate, return `SHORTCUT_CONFLICT` with the exact conflicting shortcut, cell ID, zero-based coordinates, sound ID, and sound display name;
4. if it equals this cell's current registered shortcut, return the current snapshot as an idempotent success;
5. attempt to register the new shortcut before discarding the old one;
6. if OS registration fails, return `SHORTCUT_UNAVAILABLE`; state and old registration remain unchanged;
7. persist the new assignment;
8. if persistence fails, unregister the new shortcut and retain/re-register the old shortcut;
9. publish new mappings and unregister the old shortcut only after the commit.

The callback must never observe a newly registered shortcut without a valid staged mapping. Suppress it until commit or install a staged mapping that is removed on rollback.

`clear_shortcut` unregisters first, persists, and restores registration on persistence failure.

Internal conflict details must use this JSON shape:

```json
{
  "shortcut": { "modifiers": ["ALT"], "code": "KeyF", "display": "Alt + F" },
  "conflict": {
    "cellId": "r1c2",
    "row": 1,
    "column": 2,
    "soundId": "uuid",
    "soundName": "Air horn"
  }
}
```

## 9. Grid resizing

`resize_grid` validates both dimensions as 1–12. Expansion preserves every assignment at the same coordinate. During shrink, assignments already inside the requested bounds retain their coordinates. Outside assignments are sorted by their old row-major coordinates and moved into empty target cells in row-major order. Their sounds, IDs, shortcuts, cache entries, and managed files remain unchanged; shortcut runtime targets are updated after persistence commits.

Shrinking is rejected only when the number of occupied sounds exceeds the requested cell capacity. Resizing never deletes a sound.

Return `GRID_SHRINK_BLOCKED` with every blocker, sorted row-major:

```json
{
  "requested": { "rows": 2, "columns": 2 },
  "blockingCells": [
    { "cellId": "r3c0", "row": 3, "column": 0, "soundId": "uuid", "soundName": "Air horn" }
  ]
}
```

If capacity is sufficient, persist every relocation before publishing and return the new full snapshot.

## 10. Normative IPC and event contract

All DTO fields use camelCase through `#[serde(rename_all = "camelCase")]`. Tauri command errors serialize as `ApiError`; do not return debug strings as the public error shape.

```rust
// Shape, not required module placement.
struct ApiError {
    code: String,
    message: String,
    details: Option<serde_json::Value>,
}
```

Commands and exact successful results:

| Command | Arguments | Successful result |
| --- | --- | --- |
| `get_state` | none | `AppSnapshot` |
| `set_shortcut_capture_active` | `{ active }` | `()` / JavaScript `null` |
| `pick_and_import_sound` | `{ cellId }` | `AppSnapshot \| null` |
| `pick_and_replace_sound` | `{ cellId }` | `AppSnapshot \| null` |
| `play_sound` | `{ cellId, trigger: "pointer" \| "keyboard" }` | `{ instanceId: string }` |
| `delete_sound` | `{ cellId }` | `AppSnapshot` |
| `set_shortcut` | `{ cellId, shortcut: { modifiers, code } }` | `AppSnapshot` |
| `clear_shortcut` | `{ cellId }` | `AppSnapshot` |
| `resize_grid` | `{ rows, columns }` | `AppSnapshot` |

`AppSnapshot` is:

```json
{
  "schemaVersion": 1,
  "grid": { "rows": 4, "columns": 4, "min": 1, "max": 12 },
  "cells": [
    {
      "cellId": "r0c0",
      "row": 0,
      "column": 0,
      "sound": null
    }
  ],
  "warnings": []
}
```

The backend expands empty cells and returns `cells` in row-major order. A non-null sound has:

```json
{
  "id": "uuid",
  "displayName": "Air horn",
  "format": "mp3",
  "durationMs": 1530,
  "shortcut": {
    "modifiers": ["ALT"],
    "code": "KeyF",
    "display": "Alt + F"
  },
  "shortcutStatus": "registered",
  "playable": true,
  "problem": null
}
```

`shortcutStatus` is `registered`, `unavailable`, `invalid`, or null when no shortcut exists. `problem` is null or `{ code, message }`.

Events:

```text
soundboard://playback-started
{ instanceId, soundId, cellId, trigger: pointer|keyboard|globalShortcut, startedAtMs }

soundboard://playback-failed
{ soundId|null, cellId|null, code, message }
```

Emit to the main window/app handle without requiring the window to be focused.

## 11. Public error catalog

Use stable machine-readable codes:

| Code | Meaning / required details |
| --- | --- |
| `NOT_FOUND` | Requested cell/sound does not exist |
| `CELL_EMPTY` | Operation requires an occupied cell |
| `CELL_OCCUPIED` | Import raced with another mutation |
| `UNSUPPORTED_FORMAT` | Extension/container/codec is not guaranteed |
| `FILE_TOO_LARGE` | Include `maxBytes` and observed `bytes` |
| `AUDIO_DECODE_FAILED` | Chosen file cannot be decoded; no internal path in message |
| `AUDIO_DEVICE_UNAVAILABLE` | Default output cannot initialize |
| `PLAYBACK_LIMIT_REACHED` | Configured simultaneous-voice capacity is exhausted |
| `SHORTCUT_INVALID` | Include a user-safe `reason` |
| `SHORTCUT_CONFLICT` | Include the exact shape in Section 8.3 |
| `SHORTCUT_UNAVAILABLE` | Include requested shortcut; no external app identity |
| `GRID_INVALID` | Include min/max and requested values |
| `GRID_SHRINK_BLOCKED` | Include all blockers from Section 9 |
| `PERSISTENCE_FAILED` | Mutation did not commit |
| `STATE_VERSION_UNSUPPORTED` | Persisted schema is newer; state remains untouched |
| `INTERNAL` | Unexpected failure with a correlation ID, not a stack trace |

Messages are concise and safe for display. Log technical context locally, but never log audio contents, raw shortcuts as keystroke history, or source paths at info level.

## 12. Startup sequence

1. Resolve/create the app data and audio directories.
2. Load, recover, migrate, and validate persisted state.
3. Initialize the audio worker and default device.
4. Preload each referenced audio file; record per-cell warnings without aborting unrelated cells.
5. Initialize the global-shortcut plugin and register valid unique shortcuts.
6. Construct the coordinator and expose Tauri commands.
7. Open the main window.

The frontend's first `get_state` must see complete runtime `playable` and `shortcutStatus` values. Do not race initialization after the window becomes interactive.

## 13. Security and packaging

- Use least-privilege Tauri v2 capabilities. The frontend needs only invocation/event access required by these commands; it does not need shell, HTTP, opener, arbitrary filesystem, or JavaScript global-shortcut permissions.
- Run the native dialog and all file operations in Rust.
- Set a restrictive content security policy with no remote origins and no runtime remote assets.
- Validate every command argument even if the UI already validated it.
- Do not follow a stored filename outside `audio/`; canonicalize/validate before reads or deletes.
- Use a release profile appropriate for a small app (`lto`, reduced codegen units, symbol stripping where supported, and panic abort if compatible with Tauri/plugin requirements). Measure the packaged artifact and avoid speculative dependencies.
- Configure platform bundle metadata and a non-placeholder reverse-DNS bundle identifier before release.

## 14. Tests and acceptance criteria

### 14.1 Automated Rust tests

Cover at least:

1. cell ID parsing and grid bounds;
2. hotkey modifier ordering, sided-modifier normalization, physical-code mapping, and invalid chords;
3. exact internal conflict details;
4. idempotent shortcut reassignment;
5. OS registration failure leaves old shortcut/state intact;
6. persistence failure rolls back import, replace, delete, and shortcut changes;
7. schema v1 round-trip and malformed/duplicate-state rejection;
8. recovery from current/next/previous JSON candidates;
9. corrupt state backup without overwriting an unknown future schema;
10. import size, extension, decoder, safe-name, and cancellation behavior;
11. replacement retains shortcut and old state on failure;
12. deletion never touches the source path;
13. grid expansion preservation, row-major shrink relocation, shortcut retargeting, and complete capacity blockers;
14. shortcut capture suppresses playback and suspends/restores native registrations;
15. missing/corrupt audio affects only its own cell;
16. 20 rapid plays create 20 independent service requests;
17. release events contain the correct trigger and IDs.

Use temporary directories and fake audio/hotkey/dialog adapters. Tests must not register real global shortcuts or play through the developer's speakers.

### 14.2 Manual integration matrix

On each supported OS, verify:

- import and playback for one sample of each guaranteed format;
- left-click and keyboard activation;
- the same sound triggered rapidly overlaps and does not restart/cut off;
- global shortcut works with the window unfocused and minimized;
- duplicate internal shortcut identifies the correct cell and sound;
- shortcut capture works after window reactivation, does not record while inactive, does not play assigned sounds, and bare Enter saves the selected chord;
- an externally reserved shortcut reports unavailable without naming an owner;
- restart restores grid, sounds, and shortcuts after the original imports are moved/deleted;
- replace keeps the shortcut; delete unregisters it;
- grid shrink relocates sounds when capacity exists; a capacity-blocked shrink reports every affected outside cell;
- closing the app releases registered shortcuts;
- no audio device and a missing managed file produce recoverable UI states.

The backend is done when `cargo fmt --check`, strict Clippy, tests, and release build pass; the packaged app needs no network access; the IPC contract exactly matches Section 10; and the mutation rollback guarantees above are demonstrated by tests.

## 15. Explicit non-goals

Do not implement audio recording, microphone injection, virtual audio devices, selectable output devices, per-sound volume, trimming, looping, pause/stop, stop-all, streaming long files, drag-and-drop, cell reordering, multiple boards, cloud sync, tray mode, autostart, or automatic updates in this MVP.
