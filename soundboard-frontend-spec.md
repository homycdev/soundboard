# Desktop Soundboard — UI/Frontend Implementation Specification

Version: 1.1
Status: implemented desktop release
Companion document: `soundboard-backend-spec.md`

## 1. Purpose and agent boundary

Build the complete dark, minimal desktop UI for a Tauri v2 soundboard. The UI displays a configurable grid of sound cells, lets the user import and manage audio, plays a sound on left-click, captures global-shortcut choices, and explains conflicts precisely.

The frontend agent owns:

- `src/**`, `static/**`, and `index.html`;
- the Svelte/Vite/TypeScript configuration and frontend dependencies;
- UI unit/component tests;
- a typed Tauri bridge and a mock bridge used by tests and browser-only development.

The frontend agent must not implement audio, direct filesystem access, persistence, or global shortcut registration. Those belong to the Rust backend. Do not expose source or stored filesystem paths in the UI.

## 2. Fixed product decisions

These decisions remove ambiguity for the MVP:

- Desktop targets are Windows, macOS, and Linux.
- On macOS, the main Tauri webview accepts the first mouse event so one tap on an inactive window both activates it and invokes the tapped cell or grid-settings control. Shortcut recording still ignores input while the app is inactive.
- Shortcuts are global: they work while the process is running even when the window is unfocused or minimized, except while the shortcut-capture modal intentionally suspends them.
- Closing the window quits the app. System tray behavior and autostart are not part of the MVP.
- Audio plays through the operating system's current default output device. Optional virtual-microphone routing duplicates clips to an installed virtual audio device and passes through a selected physical microphone.
- Imported files are copied into app-owned storage, so moving or deleting the original file does not break the cell.
- Guaranteed import formats are MP3, WAV, OGG/Vorbis, and FLAC.
- The default grid is 4 columns by 4 rows. Each dimension is configurable from 1 through 12.
- A cell contains at most one sound. Grid positions are stable by row and column.
- Repeated triggers start overlapping playback instances. A new trigger never stops an earlier instance.
- The app is dark-only in the MVP and contains no analytics, account, network, or cloud features.

## 3. Frontend stack

Use:

- Svelte 5 with TypeScript;
- Vite;
- the Tauri v2 JavaScript API only for `invoke` and event listening;
- Vitest and Testing Library for component tests.

Do not add a UI component framework, CSS-in-JS runtime, remote font, or icon pack. Use semantic HTML, local CSS, and small inline SVGs where an icon is useful. Keep production JavaScript and dependency count low.

Suggested structure:

```text
src/
  App.svelte
  app.css
  lib/
    api/
      bridge.ts
      contract.ts
      mockBridge.ts
    components/
      AppHeader.svelte
      SoundGrid.svelte
      SoundCell.svelte
      CellMenu.svelte
      ShortcutDialog.svelte
      GridSettingsDialog.svelte
      ConfirmDialog.svelte
      ToastRegion.svelte
    state/
      soundboardStore.ts
    hotkeys/
      capture.ts
      display.ts
```

Equivalent organization is acceptable, but the bridge and contract types must remain isolated from presentation components.

## 4. Main window and visual language

### 4.1 Window layout

The window contains only:

1. a compact header with the app name on the left and a `Grid 4 × 4` settings button on the right;
2. a scrollable main region containing the sound grid;
3. transient dialogs, the cell context menu, and a bottom-right toast region.

Recommended native-window defaults are 720 × 680 px with a 480 × 420 px minimum. The UI must remain usable at the minimum size and at 200% display scaling. Keep the native title bar/decorations.

### 4.2 Design tokens

Use CSS custom properties and this baseline palette:

```css
--bg: #101114;
--surface: #17191d;
--surface-hover: #1e2127;
--surface-active: #252936;
--border: #2b2f37;
--border-strong: #3a404b;
--text: #f2f4f7;
--text-muted: #9aa3ad;
--accent: #8b5cf6;
--accent-hover: #9d75f8;
--danger: #ef4444;
--warning: #f59e0b;
--focus: #a78bfa;
```

Use the platform system font stack, 8 px spacing increments, 10–12 px corner radii, restrained 120–180 ms transitions, and no decorative gradients. Focus rings must be visible and must not rely on color alone.

### 4.3 Grid and cells

- Render columns from `snapshot.grid.columns` and rows from `snapshot.grid.rows`.
- A cell uses `aspect-ratio: 1`, has a practical minimum of 96 px, and grows evenly when space permits. Larger grids may scroll; cells must not collapse into unusable targets.
- Empty cells have a dashed border, a plus symbol, and the label `Add sound`.
- Filled cells show the sound's display name centered and truncated to two lines. The full name appears in a tooltip.
- If assigned, the formatted shortcut appears in a small badge along the lower edge.
- A sound that is not playable or whose shortcut could not be registered shows a warning indicator with an accessible tooltip.
- A `soundboard://playback-started` event creates a fresh halo animation instance for the corresponding cell. Every event—including repeated touch/click and global-shortcut calls during an existing halo—replaces the per-cell keyed animation instance so the effect visibly restarts; do not reuse one CSS animation class/boolean.
- Pending import/replace/delete operations disable only the affected cell and show a compact spinner. Do not block the entire grid.

## 5. Required interactions

### 5.1 Initial load

On mount:

1. subscribe to both backend events before requesting state;
2. call `get_state`;
3. render a quiet skeleton until it resolves;
4. render all startup warnings as non-blocking toasts and persistent cell indicators where applicable.

If initial state cannot load, show an in-window error state with `Retry` and a concise diagnostic message. Do not render a pretend empty grid.

### 5.2 Empty cell

A primary click, Enter, or Space on an empty cell calls `pick_and_import_sound({ cellId })`.

- The Rust side opens the native single-file picker.
- A `null` response means the user cancelled; make no change and show no error.
- A snapshot response replaces the entire local snapshot.
- Import and decoding errors appear as a toast while the cell remains empty.

### 5.3 Filled cell

A primary click, Enter, or Space calls `play_sound({ cellId })`. Do not stop, toggle, or debounce existing playback. The UI waits for the playback event for the definitive pulse, though it may apply an immediate pressed style while the pointer is down.

A secondary click opens a custom context menu anchored to that cell. The native browser context menu must be suppressed only inside grid cells. The menu contains, in this order:

1. `Change shortcut` when assigned, otherwise `Set shortcut`;
2. `Remove shortcut` only when assigned;
3. `Replace sound`;
4. separator;
5. `Delete sound` in the danger style.

Also open the menu with Shift+F10 or the keyboard Context Menu key. Close it on Escape, outside click, window blur, item selection, or when its cell disappears. Reposition it so it never clips outside the viewport.

### 5.4 Replace sound

`Replace sound` calls `pick_and_replace_sound({ cellId })`.

- Cancellation is silent.
- Replacement keeps the existing shortcut.
- Until success, the old sound remains visible and playable.
- On success, replace the full snapshot.

### 5.5 Delete sound

Open a confirmation dialog containing the display name and, when present, its shortcut. Confirming calls `delete_sound({ cellId })`. On success replace the full snapshot. The dialog's initial focus is `Cancel`, not the destructive action.

### 5.6 Shortcut capture

Before opening the modal, call `set_shortcut_capture_active({ active: true })`; close it only after that command succeeds. This temporarily prevents assigned shortcuts from playing or being consumed by the OS while a replacement chord is recorded. Always call `set_shortcut_capture_active({ active: false })` when the modal closes after Save, Cancel, Escape, stale-state removal, or component teardown.

Open a modal titled `Set shortcut for <sound name>`. It contains a dedicated focusable capture field plus Cancel/Save controls. Focus the capture field initially and refocus it when the application window becomes active again. Record keys only while the application window is active. While that field is focused:

- listen to `keydown` in capture phase;
- prevent captured keystrokes from triggering cell actions;
- show currently held modifiers and the final non-modifier key;
- Escape cancels when pressed by itself;
- modifier-only input is incomplete and cannot be saved;
- normalize left/right variants of Control, Alt, Shift, and Meta into one modifier each;
- identify the non-modifier by `KeyboardEvent.code`, not localized `key` text;
- allow exactly one non-modifier key;
- require at least one modifier for letters, digits, punctuation, navigation keys, Space, Enter, Backspace, or Tab;
- allow function keys F1–F24 without a modifier;
- reject modifier-only shortcuts and keys the backend cannot represent.
- after a complete shortcut has been captured, bare Enter applies it and is not recorded as a replacement key; modified Enter remains a capturable chord.

A bare Tab keeps its normal focus-navigation behavior; modified Tab may be captured. Moving focus away from the capture field must not record dialog-control keystrokes as part of the shortcut.

Compare a captured chord with the current snapshot immediately. If it matches another cell, keep Save disabled and show that cell's sound and coordinates without triggering playback. Submit non-conflicting choices with `set_shortcut({ cellId, shortcut })`; the backend performs the authoritative conflict check again. The dialog stays open on error.

For an internal duplicate, render this exact information inline:

> **Alt + F is already assigned to “Air horn” at row 2, column 3.**

Use the current snapshot for immediate feedback and values from `error.details.conflict` if the backend reports a conflict during Save; never replace backend conflict details with a stale local guess.

For an operating-system/external conflict, say:

> **Alt + F could not be registered. It may be reserved by the operating system or another app.**

Do not claim to know which external app owns the shortcut; the operating-system API used by Tauri does not provide that identity.

`Remove shortcut` calls `clear_shortcut({ cellId })` without an extra confirmation.

### 5.7 Grid settings

The header settings button opens a modal with integer steppers/inputs for Rows and Columns. Both accept 1–12. The dialog shows a live text preview such as `6 × 4 = 24 cells`, but the actual grid changes only after `Apply` calls `resize_grid({ rows, columns })`.

When shrinking, sounds already inside the requested bounds keep their coordinates. Sounds outside the bounds move, in their existing row-major order, into empty target cells in row-major order. If the requested grid has fewer total cells than occupied sounds, the backend returns `GRID_SHRINK_BLOCKED`; keep the dialog open and list every affected outside cell by sound name and one-based row/column. Resizing must never delete a sound.

### 5.8 Virtual-microphone routing

The `Audio` header control opens a modal that lists physical microphone inputs and audio outputs. Prefer outputs identified as virtual devices and explain the platform prerequisite: BlackHole 2ch on macOS or VB-CABLE on Windows. The driver is installed separately and is never bundled or downloaded by Soundboard.

The user selects an input microphone and virtual output, adjusts microphone and soundboard gains from 0–200%, chooses whether clips are also monitored through the normal default output, and starts routing. Active, disabled, and interrupted states appear with both text and color. The modal must allow device refresh, retry, settings updates, and stopping routing. It also warns users to select the virtual device in their call app, use headphones, and disable call-app noise suppression if it removes effects.

Routing settings persist across restarts. Missing devices or denied microphone permission must not prevent ordinary soundboard management. They produce a recoverable inline error and an attention state on the header control.

## 6. Accessibility and keyboard behavior

- Every cell is a real `<button>` or has equivalent button semantics and behavior.
- Use one-based row/column in accessible labels: `Row 2, column 3, Air horn, shortcut Alt + F` or `Row 2, column 3, empty, add sound`.
- Give the grid `role="grid"`; expose rows/cells consistently and do not break normal Tab navigation.
- All dialogs trap focus, restore focus to their opener, close on Escape, and have labelled titles/descriptions.
- The context menu uses menu/menuitem semantics and arrow-key navigation.
- Toasts use `aria-live="polite"`; destructive or playback failures may use `assertive` only when necessary.
- Meet WCAG AA contrast. Never use color as the sole indication of error, focus, playback, or shortcut status.
- Honor `prefers-reduced-motion` by removing the pulse transform and retaining a brief border-color state.

## 7. Normative frontend/backend contract

JSON field names are camelCase. Rust must use matching Serde renames. `cellId` is always `r<zero-based-row>c<zero-based-column>`, for example `r0c0`.

```ts
export type CellId = string;
export type Modifier = "CONTROL" | "ALT" | "SHIFT" | "META";
export type ShortcutStatus = "registered" | "unavailable" | "invalid";

export interface ShortcutDto {
  modifiers: Modifier[]; // canonical order: CONTROL, ALT, SHIFT, META
  code: string;          // physical code, e.g. KeyF, Digit1, F8
  display: string;       // backend-authoritative platform display string
}

export interface SoundDto {
  id: string;
  displayName: string;
  format: "mp3" | "wav" | "ogg" | "flac";
  durationMs: number;
  shortcut: ShortcutDto | null;
  shortcutStatus: ShortcutStatus | null;
  playable: boolean;
  problem: { code: string; message: string } | null;
}

export interface CellDto {
  cellId: CellId;
  row: number;
  column: number;
  sound: SoundDto | null;
}

export interface AppWarningDto {
  code: string;
  message: string;
  cellId: CellId | null;
}

export interface AppSnapshot {
  schemaVersion: 1;
  grid: { rows: number; columns: number; min: 1; max: 12 };
  cells: CellDto[]; // row-major and includes empty cells
  warnings: AppWarningDto[];
}

export interface PlaybackStarted {
  instanceId: string;
  soundId: string;
  cellId: CellId;
  trigger: "pointer" | "keyboard" | "globalShortcut";
  startedAtMs: number;
}

export interface PlaybackFailed {
  soundId: string | null;
  cellId: CellId | null;
  code: string;
  message: string;
}

export interface ApiError {
  code: string;
  message: string;
  details: Record<string, unknown> | null;
}
```

Commands:

| Command | Arguments | Successful result |
| --- | --- | --- |
| `get_state` | none | `AppSnapshot` |
| `set_shortcut_capture_active` | `{ active }` | `null` |
| `pick_and_import_sound` | `{ cellId }` | `AppSnapshot \| null` |
| `pick_and_replace_sound` | `{ cellId }` | `AppSnapshot \| null` |
| `play_sound` | `{ cellId, trigger: "pointer" \| "keyboard" }` | `{ instanceId: string }` |
| `delete_sound` | `{ cellId }` | `AppSnapshot` |
| `set_shortcut` | `{ cellId, shortcut: { modifiers, code } }` | `AppSnapshot` |
| `clear_shortcut` | `{ cellId }` | `AppSnapshot` |
| `resize_grid` | `{ rows, columns }` | `AppSnapshot` |
| `get_audio_routing` | none | `AudioRoutingSnapshot` |
| `configure_audio_routing` | `{ input: AudioRoutingInput }` | `AudioRoutingSnapshot` |
| `disable_audio_routing` | none | `AudioRoutingSnapshot` |

Events:

- `soundboard://playback-started` with `PlaybackStarted`;
- `soundboard://playback-failed` with `PlaybackFailed`.

Every bridge call must catch the rejected Tauri invocation and normalize it to `ApiError`. Unknown/non-object failures become `{ code: "INTERNAL", message: "Unexpected backend error", details: null }`. Components must not call `invoke` directly.

## 8. Error presentation rules

| Error code | UI behavior |
| --- | --- |
| `SHORTCUT_CONFLICT` | Inline in shortcut modal, including exact conflicting sound and cell |
| `SHORTCUT_UNAVAILABLE` | Inline in shortcut modal; explain OS/another app may own it |
| `SHORTCUT_INVALID` | Inline in shortcut modal with corrective guidance |
| `GRID_SHRINK_BLOCKED` | Inline in grid settings with all blocking cells |
| `UNSUPPORTED_FORMAT`, `FILE_TOO_LARGE`, `AUDIO_DECODE_FAILED` | Toast; retain old/empty cell state |
| `AUDIO_DEVICE_UNAVAILABLE`, `PLAYBACK_LIMIT_REACHED` | Toast from command or playback-failed event |
| `AUDIO_INPUT_NOT_FOUND`, `VIRTUAL_OUTPUT_NOT_FOUND`, `AUDIO_ROUTING_FAILED`, `AUDIO_ROUTING_INTERRUPTED` | Inline in audio-routing modal; retain prior persisted settings |
| `PERSISTENCE_FAILED` | Error toast; retain snapshot from before the attempted mutation |
| `STATE_VERSION_UNSUPPORTED` | Full initial-load error; explain that the data was created by a newer app version and was left untouched |
| `NOT_FOUND`, `CELL_EMPTY`, `CELL_OCCUPIED`, `INTERNAL` | Error toast, then refresh with `get_state` when state may be stale |

Picker cancellation is a normal `null` result, never an error.

## 9. Frontend state and concurrency

- Treat each `AppSnapshot` as authoritative and replace local state atomically.
- Maintain per-cell pending-operation flags separately from the snapshot.
- Permit simultaneous play commands. Serialize only mutations affecting the same cell.
- Disable grid resizing while any import, replace, delete, or shortcut mutation is pending.
- Ignore a late response only if a newer mutation for the same cell has already committed; use a monotonically increasing request token per cell.
- Keep event unsubscribe functions and call them on component teardown. Development hot reload must not accumulate listeners.

## 10. Tests and acceptance criteria

Provide a mock bridge fixture and automated tests covering at least:

1. default state renders exactly 16 row-major cells;
2. empty-cell click imports; cancellation leaves state unchanged;
3. filled-cell click invokes play with the correct trigger;
4. rapid clicks issue multiple play commands and do not render a stop/toggle state;
5. right-click and keyboard invocation open the correct menu;
6. replace keeps the displayed shortcut returned by the backend;
7. delete requires confirmation and restores focus;
8. shortcut capture normalizes modifiers and uses `KeyboardEvent.code`;
9. capture resumes after window reactivation, ignores inactive-window input, and bare Enter saves a completed chord;
10. capture suspends global playback and identifies an existing shortcut immediately;
11. `SHORTCUT_CONFLICT` displays shortcut, sound name, row, and column exactly;
12. `SHORTCUT_UNAVAILABLE` does not invent an external app name;
13. grid shrink relocates outside sounds when capacity exists and lists blockers only when it does not;
14. playback events pulse the correct cell, including repeated events for one sound;
15. warning and unplayable states remain manageable through replace/delete;
16. all primary workflows work using only the keyboard;
17. no component receives or renders a filesystem path.
18. routing dialog selects a microphone and virtual output, applies gains/monitoring, reflects active state, refreshes devices, and stops routing.

The frontend is done when type-check, lint, unit/component tests, and production build pass; the app remains usable at the minimum window size and 200% scaling; and every command/event name and field matches Section 7 exactly.

## 11. Explicit non-goals

Do not add drag-and-drop, waveform rendering, per-sound volume, trimming, looping, stop-all, a bundled virtual-audio driver, microphone recording to disk, cloud sync, categories/pages, search, cell reordering, themes, tray mode, autostart, or automatic updates.
