import { stringByteAt } from "tsn:strings";

// Append a UTF-8 string view into caller-owned byte storage. Returns the next
// offset, or u32::MAX if the output slice is too small.
export function appendString(output: Uint8Array, offset: u32, value: string): u32 {
  if (offset > output.length || value.length > output.length - offset) return 4294967295;
  let index: u32 = 0;
  while (index < value.length) {
    output[offset + index] = stringByteAt(value, index);
    index++;
  }
  return offset + value.length;
}

export function appendBytes(output: Uint8Array, offset: u32, input: Uint8Array, start: u32, length: u32): u32 {
  if (start > input.length || length > input.length - start) return 4294967295;
  if (offset > output.length || length > output.length - offset) return 4294967295;
  let index: u32 = 0;
  while (index < length) {
    output[offset + index] = input[start + index];
    index++;
  }
  return offset + length;
}

// Decimal formatting is ordinary source code; a 10-byte reserve covers every
// u32, including zero.
export function appendU32Decimal(output: Uint8Array, offset: u32, value: u32): u32 {
  if (offset > output.length || output.length - offset < 10) return 4294967295;
  let remainder: u32 = value;
  let place: u32 = 1000000000;
  let step: u32 = 0;
  let started: bool = false;
  while (step < 10) {
    let digit: u32 = 0;
    while (remainder >= place) {
      remainder = remainder - place;
      digit++;
    }
    if (digit > 0 || started || place === 1) {
      output[offset] = (digit + 48) as u8;
      offset++;
      started = true;
    }
    if (step === 0) place = 100000000;
    else if (step === 1) place = 10000000;
    else if (step === 2) place = 1000000;
    else if (step === 3) place = 100000;
    else if (step === 4) place = 10000;
    else if (step === 5) place = 1000;
    else if (step === 6) place = 100;
    else if (step === 7) place = 10;
    else place = 1;
    step++;
  }
  return offset;
}

export function equalsAscii(output: Uint8Array, length: u32, expected: string): bool {
  if (length !== expected.length) return false;
  let index: u32 = 0;
  while (index < length) {
    if (output[index] !== stringByteAt(expected, index)) return false;
    index++;
  }
  return true;
}
