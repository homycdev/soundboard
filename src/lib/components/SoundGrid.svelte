<script lang="ts">
  import type { AppWarningDto, CellDto } from '../api/contract';
  import SoundCell from './SoundCell.svelte';

  export let cells: CellDto[];
  export let rows: number;
  export let columns: number;
  export let pendingCells: Record<string, boolean>;
  export let pulseVersions: Record<string, number>;
  export let warnings: AppWarningDto[];
  export let onActivate: (cell: CellDto, trigger: 'pointer' | 'keyboard') => void;
  export let onOpenMenu: (cell: CellDto, rect: DOMRect, opener: HTMLButtonElement) => void;

  $: rowGroups = Array.from({ length: rows }, (_, row) =>
    cells.filter((cell) => cell.row === row).sort((a, b) => a.column - b.column),
  );
</script>

<div class="grid-scroll-region">
  <div
    class="sound-grid"
    role="grid"
    aria-label="Sound cells"
    aria-rowcount={rows}
    aria-colcount={columns}
    style={`--grid-columns: ${columns}`}
  >
    {#each rowGroups as rowCells, row (row)}
      <div class="sound-grid-row" role="row" aria-rowindex={row + 1}>
        {#each rowCells as cell (cell.cellId)}
          <div class="sound-grid-cell" role="gridcell" aria-rowindex={cell.row + 1} aria-colindex={cell.column + 1}>
            <SoundCell
              {cell}
              pending={Boolean(pendingCells[cell.cellId])}
              pulseVersion={pulseVersions[cell.cellId] ?? 0}
              warnings={warnings.filter((warning) => warning.cellId === cell.cellId)}
              {onActivate}
              {onOpenMenu}
            />
          </div>
        {/each}
      </div>
    {/each}
  </div>
</div>
