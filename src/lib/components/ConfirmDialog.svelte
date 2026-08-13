<script lang="ts">
  import type { SoundDto } from '../api/contract';
  import DialogShell from './DialogShell.svelte';

  export let sound: SoundDto;
  export let opener: HTMLElement | null;
  export let pending = false;
  export let onCancel: () => void;
  export let onConfirm: () => void;
</script>

<DialogShell titleId="delete-title" descriptionId="delete-description" {opener} {onCancel}>
  <div class="dialog-icon danger" aria-hidden="true">
    <svg viewBox="0 0 20 20"><path d="M4 6h12M8 3h4l1 2H7l1-2Zm-2 3 1 11h6l1-11M9 9v5m2-5v5"></path></svg>
  </div>
  <h2 id="delete-title">Delete sound?</h2>
  <p id="delete-description">
    <strong>{sound.displayName}</strong> will be removed from this cell.
    {#if sound.shortcut}
      Its shortcut <kbd>{sound.shortcut.display}</kbd> will also be released.
    {/if}
  </p>
  <div class="dialog-actions">
    <button class="button secondary" type="button" data-autofocus disabled={pending} on:click={onCancel}>Cancel</button>
    <button class="button danger" type="button" disabled={pending} on:click={onConfirm}>
      {pending ? 'Deleting…' : 'Delete sound'}
    </button>
  </div>
</DialogShell>
