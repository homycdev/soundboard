<script lang="ts">
  import type { ApiError } from '../api/contract';
  import DialogShell from './DialogShell.svelte';

  interface BlockingCell {
    cellId: string;
    row: number;
    column: number;
    soundId: string;
    soundName: string;
  }

  export let initialRows: number;
  export let initialColumns: number;
  export let min: number;
  export let max: number;
  export let opener: HTMLElement | null;
  export let onCancel: () => void;
  export let onApply: (rows: number, columns: number) => Promise<void>;

  let rows = initialRows;
  let columns = initialColumns;
  let pending = false;
  let error: ApiError | null = null;

  $: isValid = Number.isInteger(rows) && Number.isInteger(columns) && rows >= min && rows <= max && columns >= min && columns <= max;
  $: unchanged = rows === initialRows && columns === initialColumns;
  $: blockingCells = blockers(error);

  function blockers(apiError: ApiError | null): BlockingCell[] {
    if (apiError?.code !== 'GRID_SHRINK_BLOCKED' || !Array.isArray(apiError.details?.blockingCells)) return [];
    return apiError.details.blockingCells.filter((item): item is BlockingCell => {
      if (typeof item !== 'object' || item === null) return false;
      const cell = item as Record<string, unknown>;
      return typeof cell.soundName === 'string' && typeof cell.row === 'number' && typeof cell.column === 'number';
    });
  }

  async function apply() {
    if (!isValid || unchanged || pending) return;
    pending = true;
    error = null;
    try {
      await onApply(rows, columns);
    } catch (caught) {
      error = caught as ApiError;
    } finally {
      pending = false;
    }
  }
</script>

<DialogShell titleId="grid-title" descriptionId="grid-description" {opener} {onCancel}>
  <div class="dialog-kicker">Layout</div>
  <h2 id="grid-title">Grid settings</h2>
  <p id="grid-description">Choose how many sound cells appear. Sounds outside a smaller grid move into its empty cells.</p>

  <div class="grid-inputs">
    <label>
      <span class="field-label">Rows</span>
      <input data-autofocus type="number" bind:value={rows} {min} {max} step="1" inputmode="numeric" />
    </label>
    <span class="dimension-symbol" aria-hidden="true">×</span>
    <label>
      <span class="field-label">Columns</span>
      <input type="number" bind:value={columns} {min} {max} step="1" inputmode="numeric" />
    </label>
  </div>
  <p class="grid-preview" aria-live="polite"><strong>{rows} × {columns}</strong> = {rows * columns} cells</p>
  {#if !isValid}
    <p class="inline-error" role="alert">Rows and columns must each be whole numbers from {min} to {max}.</p>
  {/if}

  {#if blockingCells.length}
    <div class="blocking-cells" role="alert">
      <strong>This size cannot fit every sound:</strong>
      <ul>
        {#each blockingCells as cell (cell.cellId)}
          <li>{cell.soundName} — row {cell.row + 1}, column {cell.column + 1}</li>
        {/each}
      </ul>
      <p>Choose a grid with more cells, or delete sounds first.</p>
    </div>
  {:else if error}
    <p class="inline-error" role="alert">{error.message}</p>
  {/if}

  <div class="dialog-actions">
    <button class="button secondary" type="button" disabled={pending} on:click={onCancel}>Cancel</button>
    <button class="button primary" type="button" disabled={!isValid || unchanged || pending} on:click={apply}>
      {pending ? 'Applying…' : 'Apply'}
    </button>
  </div>
</DialogShell>
