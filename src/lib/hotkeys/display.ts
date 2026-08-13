import type { Modifier, ShortcutInput } from '../api/contract';

const MODIFIER_LABELS: Record<Modifier, string> = {
  CONTROL: 'Ctrl',
  ALT: 'Alt',
  SHIFT: 'Shift',
  META: 'Meta',
};

const CODE_LABELS: Record<string, string> = {
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  ArrowUp: 'Up',
  Backquote: '`',
  Backslash: '\\',
  Backspace: 'Backspace',
  BracketLeft: '[',
  BracketRight: ']',
  Comma: ',',
  Delete: 'Delete',
  End: 'End',
  Enter: 'Enter',
  Equal: '=',
  Home: 'Home',
  Insert: 'Insert',
  Minus: '-',
  PageDown: 'Page Down',
  PageUp: 'Page Up',
  Period: '.',
  Quote: "'",
  Semicolon: ';',
  Slash: '/',
  Space: 'Space',
  Tab: 'Tab',
};

export function displayCode(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return CODE_LABELS[code] ?? code;
}

export function formatShortcut(shortcut: ShortcutInput): string {
  return [...shortcut.modifiers.map((modifier) => MODIFIER_LABELS[modifier]), displayCode(shortcut.code)].join(
    ' + ',
  );
}

export function formatModifiers(modifiers: Modifier[]): string {
  return modifiers.map((modifier) => MODIFIER_LABELS[modifier]).join(' + ');
}
