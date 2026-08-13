export type CellId = string;
export type Modifier = 'CONTROL' | 'ALT' | 'SHIFT' | 'META';
export type ShortcutStatus = 'registered' | 'unavailable' | 'invalid';

export interface ShortcutDto {
  modifiers: Modifier[];
  code: string;
  display: string;
}

export interface ShortcutInput {
  modifiers: Modifier[];
  code: string;
}

export interface SoundProblemDto {
  code: string;
  message: string;
}

export interface SoundDto {
  id: string;
  displayName: string;
  format: 'mp3' | 'wav' | 'ogg' | 'flac';
  durationMs: number;
  shortcut: ShortcutDto | null;
  shortcutStatus: ShortcutStatus | null;
  playable: boolean;
  problem: SoundProblemDto | null;
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
  cells: CellDto[];
  warnings: AppWarningDto[];
}

export interface AudioDeviceDto {
  id: string;
  name: string;
  isDefault: boolean;
  isVirtual: boolean;
}

export interface AudioRoutingSettingsDto {
  enabled: boolean;
  inputDeviceId: string | null;
  virtualOutputDeviceId: string | null;
  microphoneGainPercent: number;
  soundboardGainPercent: number;
  monitorEnabled: boolean;
  gainMax: number;
}

export interface AudioRoutingInput {
  inputDeviceId: string;
  virtualOutputDeviceId: string;
  microphoneGainPercent: number;
  soundboardGainPercent: number;
  monitorEnabled: boolean;
}

export interface AudioRoutingSnapshot {
  status: 'disabled' | 'active' | 'error';
  inputDevices: AudioDeviceDto[];
  outputDevices: AudioDeviceDto[];
  settings: AudioRoutingSettingsDto;
  error: SoundProblemDto | null;
  recommendedDriver: string;
  driverInstallUrl: string;
  driverDetected: boolean;
}

export interface PlaybackStarted {
  instanceId: string;
  soundId: string;
  cellId: CellId;
  trigger: 'pointer' | 'keyboard' | 'globalShortcut';
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

export type Unlisten = () => void;

export interface SoundboardBridge {
  getState(): Promise<AppSnapshot>;
  getAudioRouting(): Promise<AudioRoutingSnapshot>;
  configureAudioRouting(args: { input: AudioRoutingInput }): Promise<AudioRoutingSnapshot>;
  disableAudioRouting(): Promise<AudioRoutingSnapshot>;
  setShortcutCaptureActive(args: { active: boolean }): Promise<void>;
  pickAndImportSound(args: { cellId: CellId }): Promise<AppSnapshot | null>;
  pickAndReplaceSound(args: { cellId: CellId }): Promise<AppSnapshot | null>;
  playSound(args: {
    cellId: CellId;
    trigger: 'pointer' | 'keyboard';
  }): Promise<{ instanceId: string }>;
  deleteSound(args: { cellId: CellId }): Promise<AppSnapshot>;
  setShortcut(args: { cellId: CellId; shortcut: ShortcutInput }): Promise<AppSnapshot>;
  clearShortcut(args: { cellId: CellId }): Promise<AppSnapshot>;
  resizeGrid(args: { rows: number; columns: number }): Promise<AppSnapshot>;
  onPlaybackStarted(handler: (event: PlaybackStarted) => void): Promise<Unlisten>;
  onPlaybackFailed(handler: (event: PlaybackFailed) => void): Promise<Unlisten>;
}
