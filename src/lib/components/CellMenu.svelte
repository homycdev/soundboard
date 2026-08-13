<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { SoundDto } from '../api/contract';

  type CellMenuAction = 'shortcut' | 'clear-shortcut' | 'replace' | 'delete';

  export let sound: SoundDto;
  export let x: number;
  export let y: number;
  export let onAction: (action: CellMenuAction) => void;
  export let onClose: (restoreFocus?: boolean) => void;

  let menu: HTMLDivElement;
  let left = x;
  let top = y;

  function items() {
    return Array.from(menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
  }

  function handleWindowPointer(event: MouseEvent) {
    if (!menu.contains(event.target as Node)) onClose(false);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose(true);
      return;
    }

    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const menuItems = items();
    const index = menuItems.indexOf(document.activeElement as HTMLButtonElement);
    let next = index;
    if (event.key === 'ArrowDown') next = (index + 1) % menuItems.length;
    if (event.key === 'ArrowUp') next = (index - 1 + menuItems.length) % menuItems.length;
    if (event.key === 'Home') next = 0;
    if (event.key === 'End') next = menuItems.length - 1;
    menuItems[next].focus();
  }

  onMount(async () => {
    await tick();
    const rect = menu.getBoundingClientRect();
    left = Math.max(8, Math.min(x, window.innerWidth - rect.width - 8));
    top = Math.max(8, Math.min(y, window.innerHeight - rect.height - 8));
    await tick();
    items()[0]?.focus();
  });
</script>

<svelte:window on:mousedown={handleWindowPointer} on:blur={() => onClose(false)} />

<div
  class="cell-menu"
  bind:this={menu}
  role="menu"
  tabindex="-1"
  aria-label={`Actions for ${sound.displayName}`}
  style={`left: ${left}px; top: ${top}px`}
  on:keydown={handleKeydown}
>
  <button type="button" role="menuitem" on:click={() => onAction('shortcut')}>
    <span>{sound.shortcut ? 'Change shortcut' : 'Set shortcut'}</span>
    <span class="menu-hint" aria-hidden="true">⌘</span>
  </button>
  {#if sound.shortcut}
    <button type="button" role="menuitem" on:click={() => onAction('clear-shortcut')}>
      Remove shortcut
    </button>
  {/if}
  <button type="button" role="menuitem" on:click={() => onAction('replace')}>Replace sound</button>
  <div class="menu-separator" role="separator"></div>
  <button class="danger-menu-item" type="button" role="menuitem" on:click={() => onAction('delete')}>
    Delete sound
  </button>
</div>
