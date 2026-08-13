<script lang="ts">
  import { onMount } from 'svelte';
  import { createDefaultBridge, normalizeApiError } from './lib/api/bridge';
  import type {
    ApiError,
    AppSnapshot,
    AudioRoutingInput,
    AudioRoutingSnapshot,
    CellDto,
    PlaybackFailed,
    PlaybackStarted,
    ShortcutInput,
    SoundboardBridge,
  } from './lib/api/contract';
  import AppHeader from './lib/components/AppHeader.svelte';
  import AudioRoutingDialog from './lib/components/AudioRoutingDialog.svelte';
  import CellMenu from './lib/components/CellMenu.svelte';
  import ConfirmDialog from './lib/components/ConfirmDialog.svelte';
  import GridSettingsDialog from './lib/components/GridSettingsDialog.svelte';
  import ShortcutDialog from './lib/components/ShortcutDialog.svelte';
  import SoundGrid from './lib/components/SoundGrid.svelte';
  import ToastRegion from './lib/components/ToastRegion.svelte';

  export let bridge: SoundboardBridge = createDefaultBridge();

  interface Toast {
    id: number;
    message: string;
    tone: 'info' | 'warning' | 'error';
    assertive?: boolean;
  }

  interface MenuState {
    cellId: string;
    x: number;
    y: number;
    opener: HTMLButtonElement;
  }

  interface DialogState {
    cellId: string;
    opener: HTMLElement | null;
  }

  interface SettingsState {
    opener: HTMLElement | null;
  }

  type CellMenuAction = 'shortcut' | 'clear-shortcut' | 'replace' | 'delete';

  let status: 'loading' | 'ready' | 'error' = 'loading';
  let snapshot: AppSnapshot | null = null;
  let audioRouting: AudioRoutingSnapshot | null = null;
  let initialError: ApiError | null = null;
  let pendingCells: Record<string, boolean> = {};
  let pulseVersions: Record<string, number> = {};
  let requestTokens: Record<string, number> = {};
  let menu: MenuState | null = null;
  let shortcutDialog: DialogState | null = null;
  let deleteDialog: DialogState | null = null;
  let settingsDialog: SettingsState | null = null;
  let audioRoutingDialog: SettingsState | null = null;
  let toasts: Toast[] = [];
  let nextToastId = 0;
  let unlisteners: Array<() => void> = [];
  let toastTimers: Record<number, ReturnType<typeof setTimeout>> = {};
  let routingPollTimer: ReturnType<typeof setInterval> | null = null;
  let routingPollPending = false;
  let destroyed = false;

  $: menuCell = menu ? snapshot?.cells.find((cell) => cell.cellId === menu?.cellId && cell.sound) ?? null : null;
  $: shortcutCell = shortcutDialog
    ? snapshot?.cells.find((cell) => cell.cellId === shortcutDialog?.cellId && cell.sound) ?? null
    : null;
  $: deleteCell = deleteDialog
    ? snapshot?.cells.find((cell) => cell.cellId === deleteDialog?.cellId && cell.sound) ?? null
    : null;
  $: anyMutationPending = Object.values(pendingCells).some(Boolean);
  const skeletonCells = Array.from({ length: 12 }, (_, index) => index);

  function addToast(
    message: string,
    tone: Toast['tone'] = 'error',
    assertive = false,
    duration = 6000,
  ) {
    const id = ++nextToastId;
    toasts = [...toasts, { id, message, tone, assertive }];
    const timer = setTimeout(() => dismissToast(id), duration);
    toastTimers[id] = timer;
  }

  function dismissToast(id: number) {
    toasts = toasts.filter((toast) => toast.id !== id);
    const timer = toastTimers[id];
    if (timer) clearTimeout(timer);
    delete toastTimers[id];
  }

  function applySnapshot(next: AppSnapshot, announceWarnings = false) {
    snapshot = next;
    if (menu && !next.cells.some((cell) => cell.cellId === menu?.cellId && cell.sound)) menu = null;
    if (
      shortcutDialog &&
      !next.cells.some((cell) => cell.cellId === shortcutDialog?.cellId && cell.sound)
    ) {
      closeShortcutDialog();
    }
    if (deleteDialog && !next.cells.some((cell) => cell.cellId === deleteDialog?.cellId && cell.sound)) {
      deleteDialog = null;
    }
    if (announceWarnings) {
      next.warnings.forEach((warning) => addToast(warning.message, 'warning', false, 8000));
    }
  }

  function handlePlaybackStarted(event: PlaybackStarted) {
    pulseVersions = {
      ...pulseVersions,
      [event.cellId]: (pulseVersions[event.cellId] ?? 0) + 1,
    };
  }

  function handlePlaybackFailed(event: PlaybackFailed) {
    addToast(event.message, 'error', true);
  }

  function cleanupListeners() {
    unlisteners.forEach((unlisten) => unlisten());
    unlisteners = [];
  }

  async function initialize() {
    cleanupListeners();
    status = 'loading';
    initialError = null;
    try {
      const stopStarted = await bridge.onPlaybackStarted(handlePlaybackStarted);
      if (destroyed) return stopStarted();
      unlisteners.push(stopStarted);

      const stopFailed = await bridge.onPlaybackFailed(handlePlaybackFailed);
      if (destroyed) return stopFailed();
      unlisteners.push(stopFailed);

      const initialSnapshot = await bridge.getState();
      if (destroyed) return;
      applySnapshot(initialSnapshot, true);
      try {
        audioRouting = await bridge.getAudioRouting();
      } catch (error) {
        presentError(error);
      }
      status = 'ready';
    } catch (error) {
      cleanupListeners();
      initialError = normalizeApiError(error);
      status = 'error';
    }
  }

  async function refreshAfterStaleError() {
    try {
      applySnapshot(await bridge.getState());
    } catch {
      addToast('The latest soundboard state could not be refreshed.', 'error');
    }
  }

  function presentError(error: unknown, refreshWhenStale = false) {
    const apiError = normalizeApiError(error);
    addToast(apiError.message, 'error', ['AUDIO_DEVICE_UNAVAILABLE', 'PLAYBACK_LIMIT_REACHED'].includes(apiError.code));
    if (
      refreshWhenStale &&
      ['NOT_FOUND', 'CELL_EMPTY', 'CELL_OCCUPIED', 'INTERNAL'].includes(apiError.code)
    ) {
      void refreshAfterStaleError();
    }
  }

  async function runCellMutation(
    cellId: string,
    mutation: () => Promise<AppSnapshot | null>,
  ): Promise<AppSnapshot | null> {
    if (pendingCells[cellId]) return null;
    const token = (requestTokens[cellId] ?? 0) + 1;
    requestTokens = { ...requestTokens, [cellId]: token };
    pendingCells = { ...pendingCells, [cellId]: true };

    try {
      const result = await mutation();
      if (result && requestTokens[cellId] === token) applySnapshot(result);
      return result;
    } catch (error) {
      throw normalizeApiError(error);
    } finally {
      if (requestTokens[cellId] === token) {
        const next = { ...pendingCells };
        delete next[cellId];
        pendingCells = next;
      }
    }
  }

  async function importSound(cellId: string) {
    try {
      await runCellMutation(cellId, () => bridge.pickAndImportSound({ cellId }));
    } catch (error) {
      presentError(error, true);
    }
  }

  async function playSound(cellId: string, trigger: 'pointer' | 'keyboard') {
    try {
      await bridge.playSound({ cellId, trigger });
    } catch (error) {
      presentError(error, true);
    }
  }

  function activateCell(cell: CellDto, trigger: 'pointer' | 'keyboard') {
    if (pendingCells[cell.cellId]) return;
    if (cell.sound) void playSound(cell.cellId, trigger);
    else void importSound(cell.cellId);
  }

  function openCellMenu(cell: CellDto, rect: DOMRect, opener: HTMLButtonElement) {
    if (!cell.sound) return;
    menu = { cellId: cell.cellId, x: rect.left, y: rect.bottom + 6, opener };
  }

  function closeMenu(restoreFocus = false) {
    const opener = menu?.opener;
    menu = null;
    if (restoreFocus) opener?.focus();
  }

  async function replaceSound(cellId: string) {
    try {
      await runCellMutation(cellId, () => bridge.pickAndReplaceSound({ cellId }));
    } catch (error) {
      presentError(error, true);
    }
  }

  async function clearShortcut(cellId: string) {
    try {
      await runCellMutation(cellId, () => bridge.clearShortcut({ cellId }));
    } catch (error) {
      presentError(error, true);
    }
  }

  async function saveShortcut(cellId: string, shortcut: ShortcutInput) {
    try {
      await runCellMutation(cellId, () => bridge.setShortcut({ cellId, shortcut }));
      closeShortcutDialog();
    } catch (error) {
      throw normalizeApiError(error);
    }
  }

  async function openShortcutDialog(cellId: string, opener: HTMLElement | null) {
    try {
      await bridge.setShortcutCaptureActive({ active: true });
      if (
        destroyed ||
        !snapshot?.cells.some((cell) => cell.cellId === cellId && cell.sound)
      ) {
        await bridge.setShortcutCaptureActive({ active: false });
        return;
      }
      shortcutDialog = { cellId, opener };
    } catch (error) {
      presentError(error);
    }
  }

  function closeShortcutDialog() {
    if (!shortcutDialog) return;
    shortcutDialog = null;
    void bridge.setShortcutCaptureActive({ active: false }).catch(() => undefined);
  }

  async function confirmDelete(cellId: string) {
    try {
      await runCellMutation(cellId, () => bridge.deleteSound({ cellId }));
      deleteDialog = null;
    } catch (error) {
      presentError(error, true);
    }
  }

  function handleMenuAction(action: CellMenuAction) {
    if (!menu) return;
    const { cellId, opener } = menu;
    closeMenu(false);
    if (action === 'shortcut') void openShortcutDialog(cellId, opener);
    if (action === 'clear-shortcut') void clearShortcut(cellId);
    if (action === 'replace') void replaceSound(cellId);
    if (action === 'delete') deleteDialog = { cellId, opener };
  }

  async function resizeGrid(rows: number, columns: number) {
    try {
      applySnapshot(await bridge.resizeGrid({ rows, columns }));
      settingsDialog = null;
    } catch (error) {
      throw normalizeApiError(error);
    }
  }

  async function openAudioRouting(opener: HTMLElement | null) {
    audioRoutingDialog = { opener };
    try {
      audioRouting = await bridge.getAudioRouting();
    } catch (error) {
      presentError(error);
    }
  }

  async function configureAudioRouting(input: AudioRoutingInput) {
    try {
      audioRouting = await bridge.configureAudioRouting({ input });
      addToast('Your microphone and soundboard are now routed to the virtual input.', 'info');
    } catch (error) {
      throw normalizeApiError(error);
    }
  }

  async function disableAudioRouting() {
    try {
      audioRouting = await bridge.disableAudioRouting();
      addToast('Virtual-microphone routing stopped.', 'info');
    } catch (error) {
      throw normalizeApiError(error);
    }
  }

  async function refreshAudioRouting() {
    try {
      audioRouting = await bridge.getAudioRouting();
    } catch (error) {
      throw normalizeApiError(error);
    }
  }

  async function pollAudioRouting() {
    if (routingPollPending || !audioRouting?.settings.enabled) return;
    routingPollPending = true;
    const previousStatus = audioRouting.status;
    try {
      audioRouting = await bridge.getAudioRouting();
      if (previousStatus === 'active' && audioRouting.status === 'error') {
        addToast(audioRouting.error?.message ?? 'Audio routing stopped unexpectedly.', 'error', true);
      }
    } catch {
      // An explicit refresh still reports enumeration errors in the dialog.
    } finally {
      routingPollPending = false;
    }
  }

  function initialDiagnostic() {
    if (!initialError) return '';
    if (initialError.code === 'STATE_VERSION_UNSUPPORTED') {
      return 'Your soundboard data was created by a newer app version. It was left untouched. Update Soundboard, then try again.';
    }
    return initialError.message;
  }

  onMount(() => {
    void initialize();
    routingPollTimer = setInterval(() => void pollAudioRouting(), 5000);
    return () => {
      destroyed = true;
      if (shortcutDialog) void bridge.setShortcutCaptureActive({ active: false });
      cleanupListeners();
      Object.values(toastTimers).forEach((timer) => clearTimeout(timer));
      toastTimers = {};
      if (routingPollTimer) clearInterval(routingPollTimer);
      routingPollTimer = null;
    };
  });
</script>

<div class="app-shell">
  {#if status === 'ready' && snapshot}
    <AppHeader
      rows={snapshot.grid.rows}
      columns={snapshot.grid.columns}
      disabled={anyMutationPending}
      routingStatus={audioRouting?.status ?? 'disabled'}
      onOpenSettings={(opener) => (settingsDialog = { opener })}
      onOpenAudioRouting={openAudioRouting}
    />
    <main>
      <SoundGrid
        cells={snapshot.cells}
        rows={snapshot.grid.rows}
        columns={snapshot.grid.columns}
        {pendingCells}
        {pulseVersions}
        warnings={snapshot.warnings}
        onActivate={activateCell}
        onOpenMenu={openCellMenu}
      />
    </main>
  {:else if status === 'error'}
    <header class="app-header error-header">
      <div class="brand"><span class="brand-mark" aria-hidden="true"><i></i><i></i><i></i><i></i></span><h1>Soundboard</h1></div>
    </header>
    <main class="initial-state">
      <div class="initial-state-card" role="alert">
        <div class="dialog-icon warning" aria-hidden="true">!</div>
        <h2>Soundboard couldn’t load</h2>
        <p>{initialDiagnostic()}</p>
        <button class="button primary" type="button" on:click={initialize}>Retry</button>
      </div>
    </main>
  {:else}
    <header class="app-header skeleton-header" aria-hidden="true">
      <span class="skeleton skeleton-brand"></span>
      <span class="skeleton skeleton-button"></span>
    </header>
    <main class="skeleton-grid" aria-label="Loading soundboard" aria-busy="true">
      {#each skeletonCells as index (index)}
        <span class="skeleton skeleton-cell"></span>
      {/each}
    </main>
  {/if}
</div>

{#if menu && menuCell?.sound}
  <CellMenu
    sound={menuCell.sound}
    x={menu.x}
    y={menu.y}
    onAction={handleMenuAction}
    onClose={closeMenu}
  />
{/if}

{#if shortcutDialog && shortcutCell?.sound}
  <ShortcutDialog
    sound={shortcutCell.sound}
    cells={snapshot?.cells ?? []}
    opener={shortcutDialog.opener}
    onCancel={closeShortcutDialog}
    onSave={(shortcut) => saveShortcut(shortcutCell.cellId, shortcut)}
  />
{/if}

{#if deleteDialog && deleteCell?.sound}
  <ConfirmDialog
    sound={deleteCell.sound}
    opener={deleteDialog.opener}
    pending={Boolean(pendingCells[deleteCell.cellId])}
    onCancel={() => (deleteDialog = null)}
    onConfirm={() => void confirmDelete(deleteCell.cellId)}
  />
{/if}

{#if settingsDialog && snapshot}
  <GridSettingsDialog
    initialRows={snapshot.grid.rows}
    initialColumns={snapshot.grid.columns}
    min={snapshot.grid.min}
    max={snapshot.grid.max}
    opener={settingsDialog.opener}
    onCancel={() => (settingsDialog = null)}
    onApply={resizeGrid}
  />
{/if}

{#if audioRoutingDialog && audioRouting}
  <AudioRoutingDialog
    snapshot={audioRouting}
    opener={audioRoutingDialog.opener}
    onCancel={() => (audioRoutingDialog = null)}
    onApply={configureAudioRouting}
    onDisable={disableAudioRouting}
    onRefresh={refreshAudioRouting}
  />
{/if}

<ToastRegion {toasts} onDismiss={dismissToast} />
