import { describe, expect, it } from 'vitest';
import { normalizeApiError } from './bridge';

describe('bridge error normalization', () => {
  it('keeps a structured backend error', () => {
    const error = normalizeApiError({ code: 'CELL_EMPTY', message: 'Cell is empty.', details: { cellId: 'r0c0' } });
    expect(error).toEqual({ code: 'CELL_EMPTY', message: 'Cell is empty.', details: { cellId: 'r0c0' } });
  });

  it('replaces unknown failures with the safe internal error', () => {
    expect(normalizeApiError(new Error('/private/user/audio.mp3'))).toEqual({
      code: 'INTERNAL',
      message: 'Unexpected backend error',
      details: null,
    });
  });
});
