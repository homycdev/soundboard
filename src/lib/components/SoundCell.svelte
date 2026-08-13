<script lang="ts">
  import type { AppWarningDto, CellDto } from '../api/contract';

  export let cell: CellDto;
  export let pending = false;
  export let pulseVersion = 0;
  export let warnings: AppWarningDto[] = [];
  export let onActivate: (cell: CellDto, trigger: 'pointer' | 'keyboard') => void;
  export let onOpenMenu: (cell: CellDto, rect: DOMRect, opener: HTMLButtonElement) => void;

  let button: HTMLButtonElement;

  $: sound = cell.sound;
  $: warningMessages = [
    ...warnings.map((warning) => warning.message),
    ...(sound?.problem ? [sound.problem.message] : []),
    ...(sound?.shortcutStatus === 'unavailable'
      ? ['This shortcut could not be registered. It may be reserved by the operating system or another app.']
      : []),
    ...(sound?.shortcutStatus === 'invalid' ? ['This shortcut is invalid and needs to be changed.'] : []),
  ];
  $: hasWarning = Boolean(sound && (!sound.playable || sound.shortcutStatus === 'unavailable' || sound.shortcutStatus === 'invalid')) || warningMessages.length > 0;
  $: accessibleLabel = sound
    ? `Row ${cell.row + 1}, column ${cell.column + 1}, ${sound.displayName}${sound.shortcut ? `, shortcut ${sound.shortcut.display}` : ''}${hasWarning ? ', warning' : ''}`
    : `Row ${cell.row + 1}, column ${cell.column + 1}, empty, add sound`;

  function handleClick(event: MouseEvent) {
    onActivate(cell, event.detail === 0 ? 'keyboard' : 'pointer');
  }

  function openMenu(event: MouseEvent | KeyboardEvent) {
    event.preventDefault();
    if (!sound || pending) return;
    onOpenMenu(cell, button.getBoundingClientRect(), button);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.code === 'ContextMenu' || (event.shiftKey && event.code === 'F10')) {
      openMenu(event);
    }
  }

</script>

<div class="sound-cell-frame">
  {#if pulseVersion > 0}
    {#key pulseVersion}
      <span class="sound-cell-halo" data-pulse-version={pulseVersion} aria-hidden="true"></span>
    {/key}
  {/if}
  <button
    class:empty={!sound}
    class:filled={Boolean(sound)}
    class:has-warning={hasWarning}
    class="sound-cell"
    type="button"
    bind:this={button}
    disabled={pending}
    aria-label={accessibleLabel}
    aria-busy={pending}
    title={sound?.displayName}
    on:click={handleClick}
    on:contextmenu={openMenu}
    on:keydown={handleKeydown}
  >
    {#if pending}
      <span class="cell-spinner" aria-hidden="true"></span>
      <span class="sr-only">Working…</span>
    {:else if sound}
      {#if hasWarning}
        <span class="warning-indicator" title={warningMessages.join(' ')} aria-label={warningMessages.join(' ')}>
          <svg viewBox="0 0 20 20" aria-hidden="true">
            <path d="M10 2.2 18 17H2L10 2.2Z"></path>
            <path d="M10 7v4.6M10 14.4v.1"></path>
          </svg>
        </span>
      {/if}
      <span class="sound-glyph" aria-hidden="true">
        <i></i><i></i><i></i><i></i><i></i>
      </span>
      <span class="sound-name">{sound.displayName}</span>
      <span class="cell-meta">
        <span class="format-label">{sound.format}</span>
        {#if sound.shortcut}
          <kbd class:shortcut-warning={sound.shortcutStatus !== 'registered'}>{sound.shortcut.display}</kbd>
        {/if}
      </span>
    {:else}
      <span class="add-icon" aria-hidden="true">+</span>
      <span class="add-label">Add sound</span>
    {/if}
  </button>
</div>
