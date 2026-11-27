function main(): i32 {
  let value: i8 = 127;
  const old: i8 = value++;
  if (old === 127 && value === -128) return 0;
  return 1;
}
