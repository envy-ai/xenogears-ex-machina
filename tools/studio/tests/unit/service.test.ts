import { describe, expect, it } from 'vitest';
import { readCommandResult } from '../../src/backend/service';

describe('readCommandResult', () => {
  it('unwraps arbitrary JSON success values without truthiness checks', () => {
    const values: unknown[] = [
      null,
      false,
      0,
      '',
      { ok: false, error: { kind: 'user-data', message: 'not a protocol error' } },
    ];

    for (const value of values) {
      expect(readCommandResult(JSON.stringify({ ok: value }))).toEqual(value);
    }
  });

  it('rejects envelopes without exactly one top-level result key', () => {
    expect(() => readCommandResult('not JSON')).toThrow(SyntaxError);

    for (const reply of [
      'null',
      '[]',
      '{}',
      '{"ok":null,"error":{"kind":"bad","message":"bad"}}',
      '{"ok":null,"extra":true}',
      '{"error":{"kind":"bad"}}',
      '{"error":{"message":"bad"}}',
      '{"error":{"kind":"bad","message":"bad","stack":"unexpected"}}',
      '{"error":{"kind":"bad","message":3}}',
    ]) {
      expect(() => readCommandResult(reply), reply).toThrow('Invalid studio response envelope');
    }
  });

  it('turns only a top-level protocol error into a thrown failure', () => {
    expect(() => readCommandResult(
      '{"error":{"kind":"invalid_args","message":"missing args"}}',
    )).toThrow(/invalid_args.*missing args/);

    expect(readCommandResult(JSON.stringify({
      ok: { error: { kind: 'user-data', message: 'ordinary value' } },
    }))).toEqual({ error: { kind: 'user-data', message: 'ordinary value' } });
  });
});
