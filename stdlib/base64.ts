import { stringByteAt } from "tsn:strings";

function sextet(value: u8): u32 {
  if (value >= 65 && value <= 90) return value - 65;
  if (value >= 97 && value <= 122) return value - 71;
  if (value >= 48 && value <= 57) return value + 4;
  if (value === 45) return 62;
  if (value === 95) return 63;
  return 4294967295;
}

// Decode canonical unpadded base64url into caller-owned storage. Returns the
// decoded byte count, or u32::MAX for malformed input or an undersized buffer.
export function decodeBase64Url(value: string, output: Uint8Array): u32 {
  let accumulator: u32 = 0;
  let bits: u32 = 0;
  let written: u32 = 0;
  let moduloFour: u32 = 0;
  let index: u32 = 0;
  const one: u32 = 1;
  while (index < value.length) {
    const digit: u32 = sextet(stringByteAt(value, index));
    if (digit === 4294967295) return 4294967295;
    accumulator = (accumulator << 6) | digit;
    bits = bits + 6;
    moduloFour++;
    if (moduloFour === 4) moduloFour = 0;
    if (bits >= 8) {
      bits = bits - 8;
      if (written >= output.length) return 4294967295;
      output[written] = (accumulator >>> bits) as u8;
      written++;
      if (bits === 0) {
        accumulator = 0;
      } else {
        accumulator = accumulator & ((one << bits) - 1);
      }
    }
    index++;
  }
  if (moduloFour === 1 || accumulator !== 0) return 4294967295;
  return written;
}
