import { describe, expect, it } from 'vitest';
import { captureKeydown } from './capture';
import { formatShortcut } from './display';

function keydown(init: KeyboardEventInit) {
  return new KeyboardEvent('keydown', init);
}

describe('shortcut capture', () => {
  it('normalizes sided modifiers and uses the physical code', () => {
    const result = captureKeydown(
      keydown({ code: 'KeyF', key: 'а', ctrlKey: true, altKey: true, shiftKey: true }),
    );

    expect(result).toEqual({
      kind: 'shortcut',
      shortcut: { modifiers: ['CONTROL', 'ALT', 'SHIFT'], code: 'KeyF' },
    });
  });

  it('allows bare function keys but requires modifiers for ordinary keys', () => {
    expect(captureKeydown(keydown({ code: 'F12', key: 'F12' }))).toEqual({
      kind: 'shortcut',
      shortcut: { modifiers: [], code: 'F12' },
    });
    expect(captureKeydown(keydown({ code: 'Digit1', key: '1' }))).toMatchObject({ kind: 'invalid' });
  });

  it('lets bare Tab navigate and rejects unknown keys', () => {
    expect(captureKeydown(keydown({ code: 'Tab', key: 'Tab' }))).toEqual({ kind: 'navigate' });
    expect(captureKeydown(keydown({ code: 'AudioVolumeUp', key: 'AudioVolumeUp' }))).toMatchObject({
      kind: 'invalid',
    });
  });

  it('formats physical codes without localized key text', () => {
    expect(formatShortcut({ modifiers: ['CONTROL', 'SHIFT'], code: 'KeyQ' })).toBe('Ctrl + Shift + Q');
  });
});
