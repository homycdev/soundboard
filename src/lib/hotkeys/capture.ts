import type { Modifier, ShortcutInput } from '../api/contract';

const MODIFIER_CODES = new Set([
  'ControlLeft',
  'ControlRight',
  'AltLeft',
  'AltRight',
  'ShiftLeft',
  'ShiftRight',
  'MetaLeft',
  'MetaRight',
]);

const SUPPORTED_NAMED_CODES = new Set([
  'ArrowDown',
  'ArrowLeft',
  'ArrowRight',
  'ArrowUp',
  'Backquote',
  'Backslash',
  'Backspace',
  'BracketLeft',
  'BracketRight',
  'Comma',
  'Delete',
  'End',
  'Enter',
  'Equal',
  'Home',
  'Insert',
  'Minus',
  'PageDown',
  'PageUp',
  'Period',
  'Quote',
  'Semicolon',
  'Slash',
  'Space',
  'Tab',
]);

function modifiersFromEvent(event: KeyboardEvent): Modifier[] {
  const modifiers: Modifier[] = [];
  if (event.ctrlKey || event.code.startsWith('Control')) modifiers.push('CONTROL');
  if (event.altKey || event.code.startsWith('Alt')) modifiers.push('ALT');
  if (event.shiftKey || event.code.startsWith('Shift')) modifiers.push('SHIFT');
  if (event.metaKey || event.code.startsWith('Meta')) modifiers.push('META');
  return modifiers;
}

function isFunctionKey(code: string) {
  return /^F(?:[1-9]|1[0-9]|2[0-4])$/.test(code);
}

function isSupportedCode(code: string) {
  return (
    /^Key[A-Z]$/.test(code) ||
    /^Digit[0-9]$/.test(code) ||
    isFunctionKey(code) ||
    SUPPORTED_NAMED_CODES.has(code)
  );
}

export type CaptureResult =
  | { kind: 'cancel' }
  | { kind: 'navigate' }
  | { kind: 'incomplete'; modifiers: Modifier[] }
  | { kind: 'invalid'; message: string; modifiers: Modifier[] }
  | { kind: 'shortcut'; shortcut: ShortcutInput };

export function captureKeydown(event: KeyboardEvent): CaptureResult {
  const modifiers = modifiersFromEvent(event);

  if (event.code === 'Escape' && modifiers.length === 0) return { kind: 'cancel' };
  if (event.code === 'Tab' && modifiers.length === 0) return { kind: 'navigate' };
  if (MODIFIER_CODES.has(event.code)) return { kind: 'incomplete', modifiers };

  if (!isSupportedCode(event.code)) {
    return {
      kind: 'invalid',
      modifiers,
      message: 'That key cannot be used for a global shortcut.',
    };
  }

  if (!isFunctionKey(event.code) && modifiers.length === 0) {
    return {
      kind: 'invalid',
      modifiers,
      message: 'Add at least one modifier such as Ctrl, Alt, Shift, or Meta.',
    };
  }

  return { kind: 'shortcut', shortcut: { modifiers, code: event.code } };
}
