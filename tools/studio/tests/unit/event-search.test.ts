import { describe, expect, it } from 'vitest';
import { EMPTY_NAMES, findInstruction, type Instruction, type NameTable } from '../../src/events/model';

function instruction(pc: number, mnemonic: string, operandKind?: string, operand?: number): Instruction {
  const operands = operandKind === undefined ? [] : [{ kind: operandKind, offset: 1, flags: null, bit: null }];
  return {
    pc,
    opcode: mnemonic === 'jump' ? 2 : 3,
    extended: null,
    size: operands.length ? 3 : 1,
    ranges: [[pc, pc + (operands.length ? 3 : 1)]],
    spec: {
      mnemonic,
      handler: mnemonic,
      size: operands.length ? 3 : 1,
      flow: 'next',
      operands,
      effect: '',
      select: null,
      also: [],
      inner: [],
      setup: null,
    },
    operands: operandKind === undefined ? [] : [operand!],
    immediate: operandKind === undefined ? [] : [null],
    successors: [],
    skips: [],
    inner: [],
  };
}

describe('event instruction search', () => {
  const messagePc = 0x20;
  const instructions = [
    instruction(0, 'jump', 'addr', messagePc),
    instruction(messagePc, 'message_box', 'msg', 5),
    instruction(0x30, 'jump', 'addr', 0x1234),
  ];
  const names: NameTable = {
    ...EMPTY_NAMES,
    messages: { '5': 'Fei bedroom line' },
  };

  it('treats an explicit PC as an instruction address, not an earlier jump target operand', () => {
    expect(findInstruction(instructions, '@0020', names, 23)?.pc).toBe(messagePc);
    expect(findInstruction(instructions, '0x20', names, 23)?.pc).toBe(messagePc);
  });

  it('does not match a nonexistent explicit PC against an operand value', () => {
    expect(findInstruction(instructions, '@1234', names, 23)).toBeUndefined();
  });

  it('keeps mnemonic and authored-name substring search', () => {
    expect(findInstruction(instructions, 'message box', names, 23)?.pc).toBe(messagePc);
    expect(findInstruction(instructions, 'fei bedroom', names, 23)?.pc).toBe(messagePc);
  });
});
