<script lang="ts">
  import type { ApiError, CellDto, ShortcutDto, ShortcutInput, SoundDto } from '../api/contract';
  import { captureKeydown } from '../hotkeys/capture';
  import { formatModifiers, formatShortcut } from '../hotkeys/display';
  import DialogShell from './DialogShell.svelte';

  export let sound: SoundDto;
  export let cells: CellDto[];
  export let opener: HTMLElement | null;
  export let onCancel: () => void;
  export let onSave: (shortcut: ShortcutInput) => Promise<void>;

  let shortcut: ShortcutInput | null = sound.shortcut
    ? { modifiers: [...sound.shortcut.modifiers], code: sound.shortcut.code }
    : null;
  let heldModifiers = shortcut?.modifiers ?? [];
  let captureError = '';
  let apiError: ApiError | null = null;
  let pending = false;
  let captureElement: HTMLButtonElement;

  $: display = shortcut ? formatShortcut(shortcut) : formatModifiers(heldModifiers);
  $: conflict = shortcut ? findConflict(shortcut) : null;
  $: currentErrorMessage = conflict
    ? `${display} is already assigned to “${conflict.sound!.displayName}” at row ${conflict.row + 1}, column ${conflict.column + 1}.`
    : buildErrorMessage(apiError, captureError, shortcut);

  function handleCapture(event: KeyboardEvent) {
    if (!document.hasFocus()) return;
    if (
      event.code === 'Enter' &&
      shortcut &&
      !event.ctrlKey &&
      !event.altKey &&
      !event.shiftKey &&
      !event.metaKey
    ) {
      event.preventDefault();
      event.stopPropagation();
      void save();
      return;
    }
    const result = captureKeydown(event);
    if (result.kind === 'navigate') return;

    event.preventDefault();
    event.stopPropagation();
    apiError = null;

    if (result.kind === 'cancel') {
      onCancel();
    } else if (result.kind === 'incomplete') {
      shortcut = null;
      heldModifiers = result.modifiers;
      captureError = '';
    } else if (result.kind === 'invalid') {
      shortcut = null;
      heldModifiers = result.modifiers;
      captureError = result.message;
    } else {
      shortcut = result.shortcut;
      heldModifiers = result.shortcut.modifiers;
      captureError = '';
    }
  }

  async function save() {
    if (!shortcut || conflict || pending) return;
    pending = true;
    apiError = null;
    try {
      await onSave(shortcut);
    } catch (error) {
      apiError = error as ApiError;
    } finally {
      pending = false;
    }
  }

  function findConflict(candidate: ShortcutInput): CellDto | null {
    return (
      cells.find(
        (cell) =>
          cell.sound?.id !== sound.id &&
          cell.sound?.shortcut?.code === candidate.code &&
          cell.sound.shortcut.modifiers.length === candidate.modifiers.length &&
          cell.sound.shortcut.modifiers.every(
            (modifier, index) => modifier === candidate.modifiers[index],
          ),
      ) ?? null
    );
  }

  function restoreCaptureFocus() {
    queueMicrotask(() => {
      if (document.hasFocus()) captureElement?.focus();
    });
  }

  function object(value: unknown): Record<string, unknown> | null {
    return typeof value === 'object' && value !== null && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : null;
  }

  function shortcutFromDetails(error: ApiError | null): ShortcutDto | null {
    const candidate = object(error?.details?.shortcut);
    return candidate && typeof candidate.display === 'string' ? (candidate as unknown as ShortcutDto) : null;
  }

  function conflictMessage(error: ApiError): string | null {
    if (error.code !== 'SHORTCUT_CONFLICT') return null;
    const conflict = object(error.details?.conflict);
    const conflictShortcut = shortcutFromDetails(error);
    if (
      !conflict ||
      !conflictShortcut ||
      typeof conflict.soundName !== 'string' ||
      typeof conflict.row !== 'number' ||
      typeof conflict.column !== 'number'
    ) {
      return error.message;
    }
    return `${conflictShortcut.display} is already assigned to “${conflict.soundName}” at row ${conflict.row + 1}, column ${conflict.column + 1}.`;
  }

  function buildErrorMessage(
    error: ApiError | null,
    localError: string,
    currentShortcut: ShortcutInput | null,
  ): string | null {
    if (!error) return localError || null;
    const attempted =
      shortcutFromDetails(error)?.display ??
      (currentShortcut ? formatShortcut(currentShortcut) : 'That shortcut');
    if (error.code === 'SHORTCUT_CONFLICT') return conflictMessage(error);
    if (error.code === 'SHORTCUT_UNAVAILABLE') {
      return `${attempted} could not be registered. It may be reserved by the operating system or another app.`;
    }
    if (error.code === 'SHORTCUT_INVALID') {
      const reason = error.details?.reason;
      return typeof reason === 'string' ? reason : 'Choose a supported key and include a modifier.';
    }
    return error.message;
  }
</script>

<svelte:window on:focus={restoreCaptureFocus} />

<DialogShell titleId="shortcut-title" descriptionId="shortcut-description" {opener} {onCancel}>
  <div class="dialog-kicker">Global shortcut</div>
  <h2 id="shortcut-title">Set shortcut for {sound.displayName}</h2>
  <p id="shortcut-description">
    Press a key combination. It will work while Soundboard is running, even when this window is not focused.
  </p>

  <span class="field-label" id="capture-label">Shortcut</span>
  <button
    bind:this={captureElement}
    type="button"
    class="shortcut-capture"
    class:has-value={Boolean(display)}
    class:field-error={Boolean(currentErrorMessage)}
    data-autofocus
    aria-labelledby="capture-label"
    aria-describedby="capture-help shortcut-error"
    on:keydown|capture={handleCapture}
  >
    {#if display}
      <span>{display}</span>
    {:else}
      <span class="capture-placeholder">Press shortcut…</span>
    {/if}
    <span class="capture-status">{shortcut ? 'Ready' : heldModifiers.length ? 'Press another key' : 'Listening'}</span>
  </button>
  <p class="field-help" id="capture-help">Letters and most keys need Ctrl, Alt, Shift, or Meta. F1–F24 can be used alone.</p>

  {#if currentErrorMessage}
    <p class="inline-error" id="shortcut-error" role="alert">
      <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M10 2.2 18 17H2L10 2.2ZM10 7v4.5m0 2.4v.1"></path></svg>
      <strong>{currentErrorMessage}</strong>
    </p>
  {:else}
    <span id="shortcut-error"></span>
  {/if}

  <div class="dialog-actions">
    <button class="button secondary" type="button" disabled={pending} on:click={onCancel}>Cancel</button>
    <button class="button primary" type="button" disabled={!shortcut || Boolean(conflict) || pending} on:click={save}>
      {pending ? 'Saving…' : 'Save shortcut'}
    </button>
  </div>
</DialogShell>
