<script lang="ts">
  export let rows: number;
  export let columns: number;
  export let disabled = false;
  export let onOpenSettings: (opener: HTMLButtonElement) => void;

  let settingsButton: HTMLButtonElement;
</script>

<header class="app-header">
  <div class="brand">
    <span class="brand-mark" aria-hidden="true">
      <i></i><i></i><i></i><i></i>
    </span>
    <h1>Soundboard</h1>
  </div>
  <button
    class="grid-settings-trigger"
    type="button"
    bind:this={settingsButton}
    {disabled}
    title={disabled ? 'Finish the current sound change before resizing the grid' : 'Change grid size'}
    on:click={() => onOpenSettings(settingsButton)}
  >
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <rect x="2" y="2" width="4" height="4" rx="1"></rect>
      <rect x="10" y="2" width="4" height="4" rx="1"></rect>
      <rect x="2" y="10" width="4" height="4" rx="1"></rect>
      <rect x="10" y="10" width="4" height="4" rx="1"></rect>
    </svg>
    Grid {rows} × {columns}
  </button>
</header>
