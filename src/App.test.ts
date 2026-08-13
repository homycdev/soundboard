import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import type { ApiError, AppSnapshot, AudioRoutingSnapshot } from './lib/api/contract';
import {
  MockSoundboardBridge,
  createDemoRoutingSnapshot,
  createDemoSnapshot,
  createEmptySnapshot,
  createSound,
} from './lib/api/mockBridge';

async function renderApp(
  snapshot: AppSnapshot = createEmptySnapshot(),
  routing: AudioRoutingSnapshot = createDemoRoutingSnapshot(),
) {
  const bridge = new MockSoundboardBridge(snapshot, routing);
  const view = render(App, { props: { bridge } });
  await screen.findByRole('grid', { name: 'Sound cells' });
  return { bridge, view };
}

function cell(name: RegExp | string) {
  return screen.getByRole('button', { name });
}

async function openMenu(target: HTMLElement) {
  await fireEvent.contextMenu(target);
  return screen.findByRole('menu');
}

describe('Soundboard', () => {
  it('renders exactly 16 row-major cells in the default state', async () => {
    await renderApp();
    const grid = screen.getByRole('grid', { name: 'Sound cells' });
    const cells = within(grid).getAllByRole('button');

    expect(cells).toHaveLength(16);
    expect(cells[0]).toHaveAccessibleName('Row 1, column 1, empty, add sound');
    expect(cells[15]).toHaveAccessibleName('Row 4, column 4, empty, add sound');
  });

  it('configures and stops virtual-microphone routing from the Audio dialog', async () => {
    const { bridge } = await renderApp();
    const configureSpy = vi.spyOn(bridge, 'configureAudioRouting');
    const disableSpy = vi.spyOn(bridge, 'disableAudioRouting');

    await fireEvent.click(screen.getByRole('button', { name: 'Audio' }));
    const dialog = await screen.findByRole('dialog', { name: 'Audio routing' });
    expect(within(dialog).getByRole('combobox', { name: 'Your microphone' })).toHaveValue(
      'mock:built-in-mic',
    );
    expect(within(dialog).getByRole('combobox', { name: 'Virtual output' })).toHaveValue(
      'mock:blackhole',
    );

    await fireEvent.input(within(dialog).getByRole('slider', { name: 'Microphone gain' }), {
      target: { value: '85' },
    });
    await fireEvent.input(within(dialog).getByRole('slider', { name: 'Soundboard gain' }), {
      target: { value: '120' },
    });
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Start routing' }));

    expect(configureSpy).toHaveBeenCalledWith({
      input: {
        inputDeviceId: 'mock:built-in-mic',
        virtualOutputDeviceId: 'mock:blackhole',
        microphoneGainPercent: 85,
        soundboardGainPercent: 120,
        monitorEnabled: true,
      },
    });
    expect(await within(dialog).findByText('Routing is active')).toBeInTheDocument();

    await fireEvent.click(within(dialog).getByRole('button', { name: 'Done' }));
    expect(screen.queryByRole('dialog', { name: 'Audio routing' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Audio' })).toHaveClass('active');

    await fireEvent.click(screen.getByRole('button', { name: 'Audio' }));
    const reopened = await screen.findByRole('dialog', { name: 'Audio routing' });
    expect(within(reopened).getByText('Routing is active')).toBeInTheDocument();
    await fireEvent.click(within(reopened).getByRole('button', { name: 'Stop routing' }));
    expect(disableSpy).toHaveBeenCalledTimes(1);
    expect(await within(reopened).findByText('Routing is off')).toBeInTheDocument();
  });

  it('explains the external driver prerequisite when no virtual output is detected', async () => {
    const routing = createDemoRoutingSnapshot();
    routing.outputDevices = routing.outputDevices.filter((device) => !device.isVirtual);
    routing.driverDetected = false;
    await renderApp(createEmptySnapshot(), routing);

    await fireEvent.click(screen.getByRole('button', { name: 'Audio' }));
    const dialog = await screen.findByRole('dialog', { name: 'Audio routing' });
    expect(within(dialog).getByText('BlackHole 2ch was not detected.')).toBeInTheDocument();
    expect(within(dialog).getByRole('link', { name: 'Open official download page' })).toHaveAttribute(
      'href',
      'https://existential.audio/blackhole/',
    );
    expect(within(dialog).getByRole('button', { name: 'Start routing' })).toBeDisabled();
  });

  it('imports from an empty-cell click and treats picker cancellation as a no-op', async () => {
    const { bridge } = await renderApp();
    const importSpy = vi.spyOn(bridge, 'pickAndImportSound').mockResolvedValue(null);
    const first = cell('Row 1, column 1, empty, add sound');

    await fireEvent.click(first, { detail: 1 });
    expect(importSpy).toHaveBeenCalledWith({ cellId: 'r0c0' });
    await waitFor(() => expect(first).not.toBeDisabled());
    expect(first).toHaveAccessibleName('Row 1, column 1, empty, add sound');
  });

  it('plays a filled cell with the correct trigger and permits overlapping rapid clicks', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('Air horn');
    const { bridge } = await renderApp(snapshot);
    const playSpy = vi.spyOn(bridge, 'playSound');
    const target = cell('Row 1, column 1, Air horn');

    await fireEvent.click(target, { detail: 1 });
    await fireEvent.click(target, { detail: 1 });
    await fireEvent.click(target, { detail: 1 });
    expect(playSpy).toHaveBeenCalledTimes(3);
    expect(playSpy).toHaveBeenNthCalledWith(1, { cellId: 'r0c0', trigger: 'pointer' });
    expect(screen.queryByText(/stop/i)).not.toBeInTheDocument();

    await fireEvent.click(target, { detail: 0 });
    expect(playSpy).toHaveBeenLastCalledWith({ cellId: 'r0c0', trigger: 'keyboard' });
  });

  it('opens the correct menu by right-click, Shift+F10, and the Context Menu key', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('Air horn');
    await renderApp(snapshot);
    const target = cell('Row 1, column 1, Air horn');

    let menu = await openMenu(target);
    expect(within(menu).getByRole('menuitem', { name: 'Set shortcut' })).toBeInTheDocument();
    expect(within(menu).getByRole('menuitem', { name: 'Replace sound' })).toBeInTheDocument();
    await fireEvent.keyDown(menu, { key: 'Escape' });

    await fireEvent.keyDown(target, { code: 'F10', key: 'F10', shiftKey: true });
    menu = await screen.findByRole('menu');
    expect(menu).toHaveAccessibleName('Actions for Air horn');
    await fireEvent.keyDown(menu, { key: 'Escape' });

    await fireEvent.keyDown(target, { code: 'ContextMenu', key: 'ContextMenu' });
    expect(await screen.findByRole('menu')).toBeInTheDocument();
  });

  it('keeps the backend-returned shortcut when replacing a sound', async () => {
    await renderApp(createDemoSnapshot());
    const target = cell(/Row 1, column 1, Air horn, shortcut Alt \+ F/);
    const menu = await openMenu(target);
    await fireEvent.click(within(menu).getByRole('menuitem', { name: 'Replace sound' }));

    const replacement = await screen.findByRole('button', {
      name: /Row 1, column 1, Replacement sound, shortcut Alt \+ F/,
    });
    expect(within(replacement).getByText('Alt + F')).toBeInTheDocument();
  });

  it('requires delete confirmation, starts on Cancel, and restores cell focus', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('Air horn');
    await renderApp(snapshot);
    const target = cell('Row 1, column 1, Air horn');
    const menu = await openMenu(target);
    await fireEvent.click(within(menu).getByRole('menuitem', { name: 'Delete sound' }));

    expect(screen.getByRole('dialog', { name: 'Delete sound?' })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus());
    await fireEvent.click(screen.getByRole('button', { name: 'Delete sound' }));

    const empty = await screen.findByRole('button', { name: 'Row 1, column 1, empty, add sound' });
    await waitFor(() => expect(empty).toHaveFocus());
  });

  it('captures normalized modifiers and submits KeyboardEvent.code', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('Air horn');
    const { bridge } = await renderApp(snapshot);
    const shortcutSpy = vi.spyOn(bridge, 'setShortcut');
    const menu = await openMenu(cell('Row 1, column 1, Air horn'));
    await fireEvent.click(within(menu).getByRole('menuitem', { name: 'Set shortcut' }));
    const capture = await screen.findByRole('button', { name: 'Shortcut' });

    await fireEvent.keyDown(capture, { code: 'KeyF', key: 'а', ctrlKey: true, altKey: true });
    await fireEvent.click(screen.getByRole('button', { name: 'Save shortcut' }));

    expect(shortcutSpy).toHaveBeenCalledWith({
      cellId: 'r0c0',
      shortcut: { modifiers: ['CONTROL', 'ALT'], code: 'KeyF' },
    });
  });

  it('reactivates shortcut capture after window focus and ignores keys while inactive', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('Air horn');
    await renderApp(snapshot);
    const menu = await openMenu(cell('Row 1, column 1, Air horn'));
    await fireEvent.click(within(menu).getByRole('menuitem', { name: 'Set shortcut' }));
    const capture = await screen.findByRole('button', { name: 'Shortcut' });
    await waitFor(() => expect(capture).toHaveFocus());

    const hasFocus = vi.spyOn(document, 'hasFocus').mockReturnValue(false);
    await fireEvent.keyDown(capture, { code: 'KeyQ', key: 'q', ctrlKey: true });
    expect(capture).toHaveTextContent('Press shortcut…');

    hasFocus.mockReturnValue(true);
    await fireEvent.focus(window);
    await waitFor(() => expect(capture).toHaveFocus());
    await fireEvent.keyDown(capture, { code: 'KeyQ', key: 'q', ctrlKey: true });
    expect(capture).toHaveTextContent('Ctrl + Q');
    hasFocus.mockRestore();
  });

  it('suppresses global playback while capturing and identifies an existing shortcut immediately', async () => {
    const { bridge } = await renderApp(createDemoSnapshot());
    const captureModeSpy = vi.spyOn(bridge, 'setShortcutCaptureActive');
    const setShortcutSpy = vi.spyOn(bridge, 'setShortcut');
    const playSpy = vi.spyOn(bridge, 'playSound');
    const menu = await openMenu(cell('Row 1, column 2, Studio applause'));
    await fireEvent.click(within(menu).getByRole('menuitem', { name: 'Set shortcut' }));
    const capture = await screen.findByRole('button', { name: 'Shortcut' });

    expect(captureModeSpy).toHaveBeenCalledWith({ active: true });
    expect(bridge.isShortcutCaptureActive()).toBe(true);
    await fireEvent.keyDown(capture, { code: 'KeyF', key: 'f', altKey: true });

    expect(
      screen.getByText('Alt + F is already assigned to “Air horn” at row 1, column 1.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save shortcut' })).toBeDisabled();
    expect(setShortcutSpy).not.toHaveBeenCalled();
    expect(playSpy).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(captureModeSpy).toHaveBeenLastCalledWith({ active: false });
    expect(bridge.isShortcutCaptureActive()).toBe(false);
  });

  it('uses bare Enter to save the selected shortcut instead of recording Enter', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('Air horn');
    const { bridge } = await renderApp(snapshot);
    const shortcutSpy = vi.spyOn(bridge, 'setShortcut');
    const menu = await openMenu(cell('Row 1, column 1, Air horn'));
    await fireEvent.click(within(menu).getByRole('menuitem', { name: 'Set shortcut' }));
    const capture = await screen.findByRole('button', { name: 'Shortcut' });

    await fireEvent.keyDown(capture, { code: 'KeyF', key: 'f', ctrlKey: true });
    await fireEvent.keyDown(capture, { code: 'Enter', key: 'Enter' });

    await waitFor(() =>
      expect(shortcutSpy).toHaveBeenCalledWith({
        cellId: 'r0c0',
        shortcut: { modifiers: ['CONTROL'], code: 'KeyF' },
      }),
    );
    expect(screen.queryByText(/Add at least one modifier/)).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: /Set shortcut/ })).not.toBeInTheDocument();
  });

  it('renders backend conflict details exactly instead of inferring stale state', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('Target');
    const { bridge } = await renderApp(snapshot);
    vi.spyOn(bridge, 'setShortcut').mockRejectedValue({
      code: 'SHORTCUT_CONFLICT',
      message: 'Shortcut already assigned.',
      details: {
        shortcut: { modifiers: ['ALT'], code: 'KeyF', display: 'Alt + F' },
        conflict: {
          cellId: 'r1c2',
          row: 1,
          column: 2,
          soundId: 'fresh-backend-id',
          soundName: 'Air horn',
        },
      },
    } satisfies ApiError);

    const menu = await openMenu(cell('Row 1, column 1, Target'));
    await fireEvent.click(within(menu).getByRole('menuitem', { name: 'Set shortcut' }));
    await fireEvent.keyDown(screen.getByRole('button', { name: 'Shortcut' }), {
      code: 'KeyF',
      key: 'f',
      altKey: true,
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Save shortcut' }));

    expect(
      await screen.findByText('Alt + F is already assigned to “Air horn” at row 2, column 3.'),
    ).toBeInTheDocument();
  });

  it('explains an unavailable shortcut without inventing an owning application', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('Target');
    const { bridge } = await renderApp(snapshot);
    vi.spyOn(bridge, 'setShortcut').mockRejectedValue({
      code: 'SHORTCUT_UNAVAILABLE',
      message: 'Shortcut unavailable.',
      details: { shortcut: { modifiers: ['ALT'], code: 'KeyF', display: 'Alt + F' } },
    } satisfies ApiError);

    const menu = await openMenu(cell('Row 1, column 1, Target'));
    await fireEvent.click(within(menu).getByRole('menuitem', { name: 'Set shortcut' }));
    await fireEvent.keyDown(screen.getByRole('button', { name: 'Shortcut' }), {
      code: 'KeyF',
      key: 'f',
      altKey: true,
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Save shortcut' }));

    const message = await screen.findByText(
      'Alt + F could not be registered. It may be reserved by the operating system or another app.',
    );
    expect(message).toBeInTheDocument();
    expect(message).not.toHaveTextContent(/Safari|Chrome|Firefox|Discord/i);
  });

  it('moves sounds into empty cells when shrinking the grid', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('One');
    snapshot.cells[1].sound = createSound('Two');
    snapshot.cells[2].sound = createSound('Three');
    snapshot.cells[3].sound = createSound('Four');
    await renderApp(snapshot);
    await fireEvent.click(screen.getByRole('button', { name: 'Grid 4 × 4' }));
    const rows = screen.getByRole('spinbutton', { name: 'Rows' });
    const columns = screen.getByRole('spinbutton', { name: 'Columns' });
    await fireEvent.input(rows, { target: { value: '3' } });
    await fireEvent.input(columns, { target: { value: '3' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Apply' }));

    expect(await screen.findByRole('button', { name: 'Grid 3 × 3' })).toBeInTheDocument();
    expect(cell('Row 1, column 1, One')).toBeInTheDocument();
    expect(cell('Row 1, column 2, Two')).toBeInTheDocument();
    expect(cell('Row 1, column 3, Three')).toBeInTheDocument();
    expect(cell('Row 2, column 1, Four')).toBeInTheDocument();
    expect(within(screen.getByRole('grid', { name: 'Sound cells' })).getAllByRole('button')).toHaveLength(9);
  });

  it('lists every unplaceable sound only when the smaller grid lacks capacity', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('One');
    snapshot.cells[1].sound = createSound('Two');
    snapshot.cells[4].sound = createSound('Three');
    snapshot.cells[5].sound = createSound('Four');
    snapshot.cells[15].sound = createSound('Applause');
    await renderApp(snapshot);
    await fireEvent.click(screen.getByRole('button', { name: 'Grid 4 × 4' }));
    await fireEvent.input(screen.getByRole('spinbutton', { name: 'Rows' }), {
      target: { value: '2' },
    });
    await fireEvent.input(screen.getByRole('spinbutton', { name: 'Columns' }), {
      target: { value: '2' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Apply' }));

    expect(await screen.findByText('Applause — row 4, column 4')).toBeInTheDocument();
    expect(screen.getByText('This size cannot fit every sound:')).toBeInTheDocument();
    expect(screen.getByRole('dialog', { name: 'Grid settings' })).toBeInTheDocument();
  });

  it('creates a fresh halo for every touch/click and global-shortcut playback', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[0].sound = createSound('Air horn');
    snapshot.cells[1].sound = createSound('Applause');
    const { bridge } = await renderApp(snapshot);
    const target = cell('Row 1, column 1, Air horn');
    const other = cell('Row 1, column 2, Applause');
    const event = {
      instanceId: 'one',
      soundId: snapshot.cells[0].sound!.id,
      cellId: 'r0c0',
      trigger: 'globalShortcut' as const,
      startedAtMs: Date.now(),
    };

    bridge.emitPlaybackStarted(event);
    const firstHalo = await waitFor(() => {
      const halo = target.parentElement?.querySelector('[data-pulse-version="1"]');
      expect(halo).toBeInTheDocument();
      return halo;
    });
    expect(other.parentElement?.querySelector('[data-pulse-version]')).not.toBeInTheDocument();

    await fireEvent.click(target, { detail: 1 });
    const pointerHalo = await waitFor(() => {
      const halo = target.parentElement?.querySelector('[data-pulse-version="2"]');
      expect(halo).toBeInTheDocument();
      return halo;
    });
    expect(pointerHalo).not.toBe(firstHalo);

    bridge.emitPlaybackStarted({ ...event, instanceId: 'three', startedAtMs: Date.now() + 2 });
    const repeatedShortcutHalo = await waitFor(() => {
      const halo = target.parentElement?.querySelector('[data-pulse-version="3"]');
      expect(halo).toBeInTheDocument();
      return halo;
    });
    expect(repeatedShortcutHalo).not.toBe(pointerHalo);
  });

  it('keeps warning and unplayable cells manageable through the keyboard menu', async () => {
    await renderApp(createDemoSnapshot());
    const target = cell(/Row 3, column 3, Missing sample, warning/);
    target.focus();
    await fireEvent.keyDown(target, { code: 'F10', key: 'F10', shiftKey: true });
    const menu = await screen.findByRole('menu');

    expect(within(menu).getByRole('menuitem', { name: 'Replace sound' })).toBeInTheDocument();
    expect(within(menu).getByRole('menuitem', { name: 'Delete sound' })).toBeInTheDocument();
  });

  it('supports empty-cell import and menu workflows from the keyboard', async () => {
    const snapshot = createEmptySnapshot();
    snapshot.cells[1].sound = createSound('Air horn');
    const { bridge } = await renderApp(snapshot);
    const importSpy = vi.spyOn(bridge, 'pickAndImportSound').mockResolvedValue(null);
    const empty = cell('Row 1, column 1, empty, add sound');
    empty.focus();
    await fireEvent.click(empty, { detail: 0 });
    expect(importSpy).toHaveBeenCalledWith({ cellId: 'r0c0' });

    const filled = cell('Row 1, column 2, Air horn');
    filled.focus();
    await fireEvent.keyDown(filled, { code: 'ContextMenu', key: 'ContextMenu' });
    expect(await screen.findByRole('menu')).toBeInTheDocument();
  });

  it('never renders a filesystem path', async () => {
    await renderApp(createDemoSnapshot());
    expect(document.body).not.toHaveTextContent('/Users/');
    expect(document.body).not.toHaveTextContent('C:\\');
    expect(document.body).not.toHaveTextContent('/home/');
  });
});
