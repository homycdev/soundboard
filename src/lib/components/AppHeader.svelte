<script lang="ts">
  export let rows: number;
  export let columns: number;
  export let disabled = false;
  export let routingStatus: 'disabled' | 'active' | 'error' = 'disabled';
  export let onOpenSettings: (opener: HTMLButtonElement) => void;
  export let onOpenAudioRouting: (opener: HTMLButtonElement) => void;

  let settingsButton: HTMLButtonElement;
  let routingButton: HTMLButtonElement;
</script>

<header class="app-header">
  <div class="brand">
    <span class="brand-mark" aria-hidden="true">
      <i></i><i></i><i></i><i></i>
    </span>
    <h1>Soundboard</h1>
  </div>
  <div class="header-actions">
    <button
      class="header-action routing-trigger"
      class:active={routingStatus === 'active'}
      class:failed={routingStatus === 'error'}
      type="button"
      bind:this={routingButton}
      title="Configure microphone and virtual-audio routing"
      on:click={() => onOpenAudioRouting(routingButton)}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path d="M5.3 7.5V4.2a2.7 2.7 0 0 1 5.4 0v3.3a2.7 2.7 0 0 1-5.4 0Z"></path>
        <path d="M3.7 7.2a4.3 4.3 0 0 0 8.6 0M8 11.5V14M5.7 14h4.6"></path>
      </svg>
      <span class="routing-dot" aria-hidden="true"></span>
      Audio
    </button>
    <button
      class="header-action grid-settings-trigger"
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
  </div>
</header>
