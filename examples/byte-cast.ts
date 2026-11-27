function main(): i32 {
  const wide: u32 = 0x123;
  const byte: u8 = wide as u8;
  if (byte === 0x23) return 0;
  return 1;
}
