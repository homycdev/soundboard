import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  ApiError,
  AppSnapshot,
  AudioRoutingSnapshot,
  PlaybackFailed,
  PlaybackStarted,
  SoundboardBridge,
} from './contract';
import { createMockBridge } from './mockBridge';

const UNEXPECTED_ERROR: ApiError = {
  code: 'INTERNAL',
  message: 'Unexpected backend error',
  details: null,
};

export function normalizeApiError(error: unknown): ApiError {
  if (typeof error !== 'object' || error === null) return { ...UNEXPECTED_ERROR };

  const candidate = error as Record<string, unknown>;
  if (typeof candidate.code !== 'string' || typeof candidate.message !== 'string') {
    return { ...UNEXPECTED_ERROR };
  }

  const details =
    candidate.details === null ||
    (typeof candidate.details === 'object' && !Array.isArray(candidate.details))
      ? (candidate.details as Record<string, unknown> | null)
      : null;

  return { code: candidate.code, message: candidate.message, details };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normalizeApiError(error);
  }
}

export function createTauriBridge(): SoundboardBridge {
  return {
    getState: () => call<AppSnapshot>('get_state'),
    getAudioRouting: () => call<AudioRoutingSnapshot>('get_audio_routing'),
    configureAudioRouting: (args) =>
      call<AudioRoutingSnapshot>('configure_audio_routing', args),
    disableAudioRouting: () => call<AudioRoutingSnapshot>('disable_audio_routing'),
    setShortcutCaptureActive: (args) => call<void>('set_shortcut_capture_active', args),
    pickAndImportSound: (args) => call<AppSnapshot | null>('pick_and_import_sound', args),
    pickAndReplaceSound: (args) => call<AppSnapshot | null>('pick_and_replace_sound', args),
    playSound: (args) => call<{ instanceId: string }>('play_sound', args),
    deleteSound: (args) => call<AppSnapshot>('delete_sound', args),
    setShortcut: (args) => call<AppSnapshot>('set_shortcut', args),
    clearShortcut: (args) => call<AppSnapshot>('clear_shortcut', args),
    resizeGrid: (args) => call<AppSnapshot>('resize_grid', args),
    onPlaybackStarted: async (handler) => {
      try {
        return await listen<PlaybackStarted>('soundboard://playback-started', (event) =>
          handler(event.payload),
        );
      } catch (error) {
        throw normalizeApiError(error);
      }
    },
    onPlaybackFailed: async (handler) => {
      try {
        return await listen<PlaybackFailed>('soundboard://playback-failed', (event) =>
          handler(event.payload),
        );
      } catch (error) {
        throw normalizeApiError(error);
      }
    },
  };
}

export function createDefaultBridge(): SoundboardBridge {
  return window.__TAURI_INTERNALS__ ? createTauriBridge() : createMockBridge();
}
