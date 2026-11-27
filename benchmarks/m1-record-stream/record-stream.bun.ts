const RECORDS = 100_000;
const PAYLOAD_BYTES = 996;
const TOTAL_BYTES = 100_000_000;

let hash = 0x811c9dc5;
let bytes = 0;
let recordIndex = 0;
let pending = new Uint8Array(0);

for await (const chunk of Bun.stdin.stream()) {
  const joined = new Uint8Array(pending.length + chunk.length);
  joined.set(pending);
  joined.set(chunk, pending.length);
  let offset = 0;
  while (joined.length - offset >= 4) {
    const view = new DataView(joined.buffer, joined.byteOffset + offset);
    const length = view.getUint32(0, true);
    if (length !== PAYLOAD_BYTES || joined.length - offset < 4 + length) break;
    let valid = true;
    for (let i = 0; i < length; i++) {
      const value = joined[offset + 4 + i];
      const expected = (recordIndex * 17 + i * 31 + 0x5a) & 0xff;
      if (value !== expected) valid = false;
      hash = Math.imul(hash ^ value, 0x01000193) >>> 0;
    }
    if (!valid) throw new Error(`bad payload at record ${recordIndex}`);
    offset += 4 + length;
    bytes += 4 + length;
    recordIndex++;
  }
  pending = joined.slice(offset);
}

if (pending.length !== 0 || recordIndex !== RECORDS || bytes !== TOTAL_BYTES) {
  throw new Error(`invalid stream: records=${recordIndex}, bytes=${bytes}, trailing=${pending.length}`);
}
console.log(`${recordIndex}\n${bytes}\n${hash}`);
