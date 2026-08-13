<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';

  export let titleId: string;
  export let descriptionId: string | undefined = undefined;
  export let opener: HTMLElement | null = null;
  export let onCancel: () => void;

  let dialog: HTMLElement;

  function focusableElements() {
    return Array.from(
      dialog.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
      ),
    ).filter((element) => !element.hasAttribute('hidden'));
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
      return;
    }

    if (event.key !== 'Tab') return;
    const focusable = focusableElements();
    if (focusable.length === 0) {
      event.preventDefault();
      dialog.focus();
      return;
    }

    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  onMount(async () => {
    await tick();
    (dialog.querySelector<HTMLElement>('[data-autofocus]') ?? focusableElements()[0] ?? dialog).focus();
  });

  onDestroy(() => opener?.focus());
</script>

<div class="dialog-backdrop">
  <div
    class="dialog-panel"
    bind:this={dialog}
    role="dialog"
    aria-modal="true"
    aria-labelledby={titleId}
    aria-describedby={descriptionId}
    tabindex="-1"
    on:keydown={handleKeydown}
  >
    <slot />
  </div>
</div>
