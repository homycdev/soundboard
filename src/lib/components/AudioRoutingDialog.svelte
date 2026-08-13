<script lang="ts">
  import type {
    ApiError,
    AudioDeviceDto,
    AudioRoutingInput,
    AudioRoutingSnapshot,
  } from '../api/contract';
  import DialogShell from './DialogShell.svelte';

  export let snapshot: AudioRoutingSnapshot;
  export let opener: HTMLElement | null;
  export let onCancel: () => void;
  export let onApply: (input: AudioRoutingInput) => Promise<void>;
  export let onDisable: () => Promise<void>;
  export let onRefresh: () => Promise<void>;

  let inputDeviceId =
    snapshot.settings.inputDeviceId ??
    snapshot.inputDevices.find((device) => device.isDefault)?.id ??
    snapshot.inputDevices[0]?.id ??
    '';
  let virtualOutputDeviceId =
    snapshot.settings.virtualOutputDeviceId ??
    snapshot.outputDevices.find((device) => device.isVirtual)?.id ??
    '';
  let microphoneGainPercent = snapshot.settings.microphoneGainPercent;
  let soundboardGainPercent = snapshot.settings.soundboardGainPercent;
  let monitorEnabled = snapshot.settings.monitorEnabled;
  let pending: 'apply' | 'disable' | 'refresh' | null = null;
  let error: ApiError | null = null;
  let lastSnapshot = snapshot;

  $: virtualFirstOutputs = [...snapshot.outputDevices].sort(
    (left, right) => Number(right.isVirtual) - Number(left.isVirtual),
  );
  $: sameDevice = Boolean(inputDeviceId && inputDeviceId === virtualOutputDeviceId);
  $: valid = Boolean(inputDeviceId && virtualOutputDeviceId && !sameDevice);
  $: unchanged =
    snapshot.settings.enabled &&
    snapshot.settings.inputDeviceId === inputDeviceId &&
    snapshot.settings.virtualOutputDeviceId === virtualOutputDeviceId &&
    snapshot.settings.microphoneGainPercent === microphoneGainPercent &&
    snapshot.settings.soundboardGainPercent === soundboardGainPercent &&
    snapshot.settings.monitorEnabled === monitorEnabled;
  $: if (snapshot !== lastSnapshot) {
    lastSnapshot = snapshot;
    inputDeviceId =
      snapshot.settings.inputDeviceId ??
      snapshot.inputDevices.find((device) => device.isDefault)?.id ??
      snapshot.inputDevices[0]?.id ??
      '';
    virtualOutputDeviceId =
      snapshot.settings.virtualOutputDeviceId ??
      snapshot.outputDevices.find((device) => device.isVirtual)?.id ??
      '';
    microphoneGainPercent = snapshot.settings.microphoneGainPercent;
    soundboardGainPercent = snapshot.settings.soundboardGainPercent;
    monitorEnabled = snapshot.settings.monitorEnabled;
  }

  function label(device: AudioDeviceDto) {
    const suffixes = [device.isDefault ? 'default' : '', device.isVirtual ? 'virtual' : '']
      .filter(Boolean)
      .join(', ');
    return suffixes ? `${device.name} — ${suffixes}` : device.name;
  }

  async function apply() {
    if (!valid || pending || (unchanged && snapshot.status === 'active')) return;
    pending = 'apply';
    error = null;
    try {
      await onApply({
        inputDeviceId,
        virtualOutputDeviceId,
        microphoneGainPercent,
        soundboardGainPercent,
        monitorEnabled,
      });
    } catch (caught) {
      error = caught as ApiError;
    } finally {
      pending = null;
    }
  }

  async function disable() {
    if (pending || !snapshot.settings.enabled) return;
    pending = 'disable';
    error = null;
    try {
      await onDisable();
    } catch (caught) {
      error = caught as ApiError;
    } finally {
      pending = null;
    }
  }

  async function refresh() {
    if (pending) return;
    pending = 'refresh';
    error = null;
    try {
      await onRefresh();
    } catch (caught) {
      error = caught as ApiError;
    } finally {
      pending = null;
    }
  }
</script>

<DialogShell titleId="routing-title" descriptionId="routing-description" {opener} {onCancel}>
  <div class="dialog-kicker">Virtual microphone</div>
  <h2 id="routing-title">Audio routing</h2>
  <p id="routing-description">
    Combine your voice and soundboard clips in a virtual input for Discord, FaceTime, and other call apps.
  </p>

  <div class="routing-status" class:active={snapshot.status === 'active'} class:failed={snapshot.status === 'error'}>
    <span class="routing-status-dot" aria-hidden="true"></span>
    <span>
      {#if snapshot.status === 'active'}
        Routing is active
      {:else if snapshot.status === 'error'}
        Routing needs attention
      {:else}
        Routing is off
      {/if}
    </span>
  </div>

  {#if snapshot.error}
    <p class="inline-error" role="alert">{snapshot.error.message}</p>
  {/if}

  <div class="routing-fields">
    <label>
      <span class="field-label">Your microphone</span>
      <select data-autofocus bind:value={inputDeviceId} disabled={Boolean(pending)}>
        {#if snapshot.inputDevices.length === 0}
          <option value="">No microphones found</option>
        {/if}
        {#each snapshot.inputDevices as device (device.id)}
          <option value={device.id}>{label(device)}</option>
        {/each}
      </select>
    </label>

    <label>
      <span class="field-label">Virtual output</span>
      <select bind:value={virtualOutputDeviceId} disabled={Boolean(pending)}>
        <option value="">Choose a virtual output…</option>
        {#each virtualFirstOutputs as device (device.id)}
          <option value={device.id}>{label(device)}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if !snapshot.driverDetected}
    <div class="driver-callout">
      <strong>{snapshot.recommendedDriver} was not detected.</strong>
      <span>Install it, restart Soundboard if required, then refresh the device list.</span>
      {#if snapshot.driverInstallUrl}
        <a href={snapshot.driverInstallUrl} target="_blank" rel="noreferrer">Open official download page</a>
      {/if}
    </div>
  {/if}

  {#if sameDevice}
    <p class="inline-error" role="alert">
      Choose a physical microphone that is different from the virtual output to prevent feedback.
    </p>
  {/if}

  <div class="gain-fields">
    <label>
      <span class="range-label"><span>Microphone gain</span><output>{microphoneGainPercent}%</output></span>
      <input
        type="range"
        aria-label="Microphone gain"
        min="0"
        max={snapshot.settings.gainMax}
        step="5"
        bind:value={microphoneGainPercent}
        disabled={Boolean(pending)}
      />
    </label>
    <label>
      <span class="range-label"><span>Soundboard gain</span><output>{soundboardGainPercent}%</output></span>
      <input
        type="range"
        aria-label="Soundboard gain"
        min="0"
        max={snapshot.settings.gainMax}
        step="5"
        bind:value={soundboardGainPercent}
        disabled={Boolean(pending)}
      />
    </label>
  </div>

  <label class="check-field">
    <input type="checkbox" bind:checked={monitorEnabled} disabled={Boolean(pending)} />
    <span>
      <strong>Play clips through my speakers or headphones</strong>
      <small>Your microphone is never monitored locally.</small>
    </span>
  </label>

  <p class="routing-help">
    In your call app, choose the virtual device as the microphone. Use headphones to prevent feedback, and disable call-app noise suppression if it removes sound effects.
  </p>

  {#if error}
    <p class="inline-error" role="alert">{error.message}</p>
  {/if}

  <div class="dialog-actions routing-actions">
    <button class="button secondary refresh-button" type="button" disabled={Boolean(pending)} on:click={refresh}>
      {pending === 'refresh' ? 'Refreshing…' : 'Refresh devices'}
    </button>
    <span class="action-spacer"></span>
    {#if snapshot.settings.enabled}
      <button class="button secondary" type="button" disabled={Boolean(pending)} on:click={disable}>
        {pending === 'disable' ? 'Stopping…' : 'Stop routing'}
      </button>
    {/if}
    <button class="button secondary" type="button" disabled={Boolean(pending)} on:click={onCancel}>
      {snapshot.settings.enabled ? 'Done' : 'Cancel'}
    </button>
    <button
      class="button primary"
      type="button"
      disabled={!valid || Boolean(pending) || (unchanged && snapshot.status === 'active')}
      on:click={apply}
    >
      {pending === 'apply' ? 'Starting…' : snapshot.settings.enabled ? 'Update routing' : 'Start routing'}
    </button>
  </div>
</DialogShell>
