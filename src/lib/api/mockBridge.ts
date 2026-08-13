import type {
  ApiError,
  AppSnapshot,
  CellDto,
  CellId,
  PlaybackFailed,
  PlaybackStarted,
  ShortcutInput,
  SoundboardBridge,
  SoundDto,
} from './contract';
import { formatShortcut } from '../hotkeys/display';

export function createEmptySnapshot(rows = 4, columns = 4): AppSnapshot {
  const cells: CellDto[] = [];
  for (let row = 0; row < rows; row += 1) {
    for (let column = 0; column < columns; column += 1) {
      cells.push({ cellId: `r${row}c${column}`, row, column, sound: null });
    }
  }
  return { schemaVersion: 1, grid: { rows, columns, min: 1, max: 12 }, cells, warnings: [] };
}

export function createSound(
  displayName: string,
  overrides: Partial<SoundDto> = {},
): SoundDto {
  return {
    id: `mock-${displayName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    displayName,
    format: 'mp3',
    durationMs: 1450,
    shortcut: null,
    shortcutStatus: null,
    playable: true,
    problem: null,
    ...overrides,
  };
}

export function createDemoSnapshot(): AppSnapshot {
  const snapshot = createEmptySnapshot();
  snapshot.cells[0].sound = createSound('Air horn', {
    shortcut: { modifiers: ['ALT'], code: 'KeyF', display: 'Alt + F' },
    shortcutStatus: 'registered',
  });
  snapshot.cells[1].sound = createSound('Studio applause');
  snapshot.cells[4].sound = createSound('Perfect timing', {
    format: 'wav',
    shortcut: { modifiers: ['CONTROL', 'SHIFT'], code: 'Digit1', display: 'Ctrl + Shift + 1' },
    shortcutStatus: 'registered',
  });
  snapshot.cells[6].sound = createSound('Tiny notification bell', { format: 'ogg' });
  snapshot.cells[10].sound = createSound('Missing sample', {
    playable: false,
    problem: { code: 'AUDIO_DECODE_FAILED', message: 'This sound could not be loaded.' },
  });
  snapshot.warnings = [
    {
      code: 'AUDIO_DECODE_FAILED',
      message: '“Missing sample” could not be loaded. You can replace or delete it.',
      cellId: 'r2c2',
    },
  ];
  return snapshot;
}

function apiError(code: string, message: string, details: Record<string, unknown> | null = null) {
  return { code, message, details } satisfies ApiError;
}

export class MockSoundboardBridge implements SoundboardBridge {
  private snapshot: AppSnapshot;
  private shortcutCaptureActive = false;
  private playbackStartedHandlers = new Set<(event: PlaybackStarted) => void>();
  private playbackFailedHandlers = new Set<(event: PlaybackFailed) => void>();
  private instance = 0;

  constructor(snapshot: AppSnapshot = createDemoSnapshot()) {
    this.snapshot = structuredClone(snapshot);
  }

  async getState() {
    return structuredClone(this.snapshot);
  }

  async setShortcutCaptureActive({ active }: { active: boolean }) {
    this.shortcutCaptureActive = active;
  }

  isShortcutCaptureActive() {
    return this.shortcutCaptureActive;
  }

  async pickAndImportSound({ cellId }: { cellId: CellId }): Promise<AppSnapshot | null> {
    const cell = this.findCell(cellId);
    if (cell.sound) throw apiError('CELL_OCCUPIED', 'That cell already contains a sound.');
    cell.sound = createSound('New sound');
    return this.copy();
  }

  async pickAndReplaceSound({ cellId }: { cellId: CellId }): Promise<AppSnapshot | null> {
    const cell = this.findCell(cellId);
    if (!cell.sound) throw apiError('CELL_EMPTY', 'That cell is empty.');
    cell.sound = createSound('Replacement sound', {
      shortcut: cell.sound.shortcut,
      shortcutStatus: cell.sound.shortcut ? 'registered' : null,
    });
    return this.copy();
  }

  async playSound({ cellId, trigger }: { cellId: CellId; trigger: 'pointer' | 'keyboard' }) {
    const cell = this.findCell(cellId);
    if (!cell.sound) throw apiError('CELL_EMPTY', 'That cell is empty.');
    if (!cell.sound.playable) {
      throw apiError('AUDIO_DEVICE_UNAVAILABLE', 'This sound is currently unavailable.');
    }
    const instanceId = `mock-play-${++this.instance}`;
    queueMicrotask(() =>
      this.emitPlaybackStarted({
        instanceId,
        soundId: cell.sound!.id,
        cellId,
        trigger,
        startedAtMs: Date.now(),
      }),
    );
    return { instanceId };
  }

  async deleteSound({ cellId }: { cellId: CellId }) {
    const cell = this.findCell(cellId);
    if (!cell.sound) throw apiError('CELL_EMPTY', 'That cell is already empty.');
    cell.sound = null;
    return this.copy();
  }

  async setShortcut({ cellId, shortcut }: { cellId: CellId; shortcut: ShortcutInput }) {
    const cell = this.findCell(cellId);
    if (!cell.sound) throw apiError('CELL_EMPTY', 'That cell is empty.');
    const duplicate = this.snapshot.cells.find(
      (candidate) =>
        candidate.cellId !== cellId &&
        candidate.sound?.shortcut?.code === shortcut.code &&
        candidate.sound.shortcut.modifiers.join(',') === shortcut.modifiers.join(','),
    );
    if (duplicate?.sound?.shortcut) {
      throw apiError('SHORTCUT_CONFLICT', 'Shortcut already assigned.', {
        shortcut: duplicate.sound.shortcut,
        conflict: {
          cellId: duplicate.cellId,
          row: duplicate.row,
          column: duplicate.column,
          soundId: duplicate.sound.id,
          soundName: duplicate.sound.displayName,
        },
      });
    }
    cell.sound.shortcut = { ...shortcut, display: formatShortcut(shortcut) };
    cell.sound.shortcutStatus = 'registered';
    return this.copy();
  }

  async clearShortcut({ cellId }: { cellId: CellId }) {
    const cell = this.findCell(cellId);
    if (!cell.sound) throw apiError('CELL_EMPTY', 'That cell is empty.');
    cell.sound.shortcut = null;
    cell.sound.shortcutStatus = null;
    return this.copy();
  }

  async resizeGrid({ rows, columns }: { rows: number; columns: number }) {
    const occupied = this.snapshot.cells.filter((cell) => cell.sound);
    const blockers = this.snapshot.cells
      .filter((cell) => cell.sound && (cell.row >= rows || cell.column >= columns))
      .map((cell) => ({
        cellId: cell.cellId,
        row: cell.row,
        column: cell.column,
        soundId: cell.sound!.id,
        soundName: cell.sound!.displayName,
      }));
    if (occupied.length > rows * columns) {
      throw apiError('GRID_SHRINK_BLOCKED', 'The requested grid cannot fit every sound.', {
        requested: { rows, columns },
        soundCount: occupied.length,
        availableCells: rows * columns,
        blockingCells: blockers,
      });
    }

    const next = createEmptySnapshot(rows, columns);
    const moving: SoundDto[] = [];
    for (const cell of occupied) {
      if (cell.row < rows && cell.column < columns) {
        next.cells.find((candidate) => candidate.cellId === cell.cellId)!.sound = cell.sound;
      } else {
        moving.push(cell.sound!);
      }
    }
    const emptyCells = next.cells.filter((cell) => !cell.sound);
    for (const [index, sound] of moving.entries()) {
      emptyCells[index].sound = sound;
    }
    this.snapshot = next;
    return this.copy();
  }

  async onPlaybackStarted(handler: (event: PlaybackStarted) => void) {
    this.playbackStartedHandlers.add(handler);
    return () => this.playbackStartedHandlers.delete(handler);
  }

  async onPlaybackFailed(handler: (event: PlaybackFailed) => void) {
    this.playbackFailedHandlers.add(handler);
    return () => this.playbackFailedHandlers.delete(handler);
  }

  emitPlaybackStarted(event: PlaybackStarted) {
    this.playbackStartedHandlers.forEach((handler) => handler(event));
  }

  emitPlaybackFailed(event: PlaybackFailed) {
    this.playbackFailedHandlers.forEach((handler) => handler(event));
  }

  private findCell(cellId: CellId) {
    const cell = this.snapshot.cells.find((candidate) => candidate.cellId === cellId);
    if (!cell) throw apiError('NOT_FOUND', 'That cell no longer exists.');
    return cell;
  }

  private copy() {
    return structuredClone(this.snapshot);
  }
}

export function createMockBridge(snapshot?: AppSnapshot) {
  return new MockSoundboardBridge(snapshot);
}
