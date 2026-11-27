// Source-level byte utility: only primitive Uint8Array access and Math.imul
// come from the compiler/runtime; the FNV-1a algorithm is ordinary tsn code.
export function fnv1a(bytes: Uint8Array): u32 {
  let hash: u32 = 2166136261;
  let index: u32 = 0;
  while (index < bytes.length) {
    hash = Math.imul(hash ^ bytes[index], 16777619) >>> 0;
    index++;
  }
  return hash;
}

export function equalConstantTime(a: Uint8Array, b: Uint8Array, length: u32): bool {
  if (length > a.length || length > b.length) return false;
  let difference: u32 = 0;
  let index: u32 = 0;
  while (index < length) {
    difference = difference | (a[index] ^ b[index]);
    index++;
  }
  return difference === 0;
}
