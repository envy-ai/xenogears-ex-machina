/** Invented field component for view, link, name and diagnostic acceptance. */
export function syntheticEventComponent(): { bytes: number[]; declaredSize: number } {
  const actorCount = 2;
  const bytecodeOffset = 132 + actorCount * 64;
  const code = new Array<number>(49).fill(0x0e);
  // Zero event pointers are skipped only when the stream starts with ff.
  code[0] = 0xff;

  // Nested conditional flow with a shared join, matching the backend's
  // synthetic structure fixture. These bytes are invented test data.
  code.splice(1, 8, 0x02, 0, 0, 0, 0, 0, 29, 0);
  code.splice(9, 8, 0x02, 0, 0, 0, 0, 0, 21, 0);
  code.splice(18, 3, 0x01, 25, 0);
  code.splice(22, 3, 0x01, 25, 0);
  code.splice(26, 3, 0x01, 29, 0);
  code[30] = 0;

  // Actor 0 event 1 requests actor 1 event 5; event 2 writes variable slot 2.
  code.splice(31, 4, 0x08, 1, 0xa5, 0);
  code.splice(35, 4, 0x36, 5, 0, 0);
  // Actor 1 event 5 is a valid target for the request.
  code[39] = 0;
  // Event 3 preserves an outside-successor diagnostic without invalid entries.
  code.splice(40, 9, 0x02, 0, 0, 0, 0, 0, 0xf4, 0x01, 0);

  const bytes = new Array<number>(bytecodeOffset + code.length).fill(0);
  bytes[128] = actorCount;
  const entry = (actor: number, event: number, pc: number) => {
    const offset = 132 + actor * 64 + event * 2;
    bytes[offset] = pc & 0xff;
    bytes[offset + 1] = pc >>> 8;
  };
  entry(0, 0, 1);
  entry(0, 1, 31);
  entry(0, 2, 35);
  entry(0, 3, 40);
  entry(1, 5, 39);
  bytes.splice(bytecodeOffset, code.length, ...code);
  return { bytes, declaredSize: bytes.length };
}
