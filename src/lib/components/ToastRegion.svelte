<script lang="ts">
  interface Toast {
    id: number;
    message: string;
    tone: 'info' | 'warning' | 'error';
    assertive?: boolean;
  }

  export let toasts: Toast[];
  export let onDismiss: (id: number) => void;
</script>

<div class="toast-region" aria-label="Notifications">
  {#each toasts as toast (toast.id)}
    <div class:warning={toast.tone === 'warning'} class:error={toast.tone === 'error'} class="toast" role={toast.assertive ? 'alert' : 'status'}>
      <span class="toast-icon" aria-hidden="true">
        {#if toast.tone === 'info'}✓{:else}!{/if}
      </span>
      <span>{toast.message}</span>
      <button type="button" aria-label="Dismiss notification" on:click={() => onDismiss(toast.id)}>×</button>
    </div>
  {/each}
</div>
