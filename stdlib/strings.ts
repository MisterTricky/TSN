import { sliceUtf8, stringByteAt } from "tsn:strings";

// The UTF-8 slicing boundary is a runtime primitive; search policy and loops
// are implemented in tsn source. Matches are byte-based, consistent with
// string.length and valid UTF-8 scalar boundaries.
export function contains(value: string, needle: string): bool {
  return indexOf(value, needle) !== 4294967295;
}

export function indexOf(value: string, needle: string): u32 {
  if (needle.length === 0) return 0;
  if (needle.length > value.length) return 4294967295;

  let start: u32 = 0;
  const last: u32 = value.length - needle.length;
  while (start <= last) {
    const candidate: string = sliceUtf8(value, start, start + needle.length);
    if (candidate === needle) return start;
    start++;
  }
  return 4294967295;
}

export function utf8ToBytes(value: string, output: Uint8Array): u32 {
  if (output.length < value.length) return 4294967295;
  let index: u32 = 0;
  while (index < value.length) {
    output[index] = stringByteAt(value, index);
    index++;
  }
  return index;
}
