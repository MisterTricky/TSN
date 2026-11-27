import { stringByteAt as builtinStringByteAt } from "tsn:strings";

export function matchesAscii(input: Uint8Array, length: u32, start: u32, text: string): bool {
  if (start > length || text.length > length - start) return false;
  let index: u32 = 0;
  while (index < text.length) {
    if (input[start + index] !== builtinStringByteAt(text, index)) return false;
    index++;
  }
  return true;
}

export function findByte(input: Uint8Array, length: u32, start: u32, value: u8): u32 {
  if (start > length) return 4294967295;
  let index: u32 = start;
  while (index < length) {
    if (input[index] === value) return index;
    index++;
  }
  return 4294967295;
}

export function parseDecimalU32(input: Uint8Array, start: u32, end: u32): u32 {
  if (start >= end) return 4294967295;
  let value: u32 = 0;
  let index: u32 = start;
  while (index < end) {
    const digit: u8 = input[index];
    if (digit < 48 || digit > 57) return 4294967295;
    const decimal: u32 = (digit - 48) as u32;
    if (value > 429496729) return 4294967295;
    if (value === 429496729 && decimal > 5) return 4294967295;
    value = value * 10 + decimal;
    index++;
  }
  return value;
}

export function parseDecimalString(value: string, start: u32, end: u32): u32 {
  if (start >= end) return 4294967295;
  let number: u32 = 0;
  let index: u32 = start;
  while (index < end) {
    const digit: u8 = builtinStringByteAt(value, index);
    if (digit < 48 || digit > 57) return 4294967295;
    const decimal: u32 = (digit - 48) as u32;
    if (number > 429496729) return 4294967295;
    if (number === 429496729 && decimal > 5) return 4294967295;
    number = number * 10 + decimal;
    index++;
  }
  return number;
}

function isRequestIdByte(value: u8): bool {
  if (value >= 97 && value <= 122) return true;
  if (value >= 48 && value <= 57) return true;
  return value === 45;
}

function isRequestNameByte(value: u8): bool {
  if (value >= 97 && value <= 122) return true;
  if (value >= 65 && value <= 90) return true;
  if (value >= 48 && value <= 57) return true;
  if (value === 32 || value === 45 || value === 95) return true;
  return false;
}

export function parseM5RequestBody(value: Uint8Array, length: u32, fields: Uint32Array): bool {
  if (fields.length < 5 || length > value.length) return false;
  const requestPrefix: string = "{\"requestId\":\"";
  if (!matchesAscii(value, length, 0, requestPrefix)) return false;
  const requestStart: u32 = requestPrefix.length;
  const requestEnd: u32 = findByte(value, length, requestStart, 34);
  if (requestEnd === 4294967295) return false;
  const requestLength: u32 = requestEnd - requestStart;
  if (requestLength === 0 || requestLength > 24) return false;
  let index: u32 = requestStart;
  while (index < requestEnd) {
    const byte: u8 = value[index];
    if (!isRequestIdByte(byte)) return false;
    index++;
  }

  const namePrefix: string = ",\"item\":{\"name\":\"";
  const nameStart: u32 = requestEnd + 1 + namePrefix.length;
  if (!matchesAscii(value, length, requestEnd + 1, namePrefix)) return false;
  const nameEnd: u32 = findByte(value, length, nameStart, 34);
  if (nameEnd === 4294967295) return false;
  const nameLength: u32 = nameEnd - nameStart;
  if (nameLength === 0 || nameLength > 40) return false;
  let nameIndex: u32 = nameStart;
  while (nameIndex < nameEnd) {
    if (!isRequestNameByte(value[nameIndex])) return false;
    nameIndex++;
  }

  const quantityPrefix: string = ",\"quantity\":";
  if (!matchesAscii(value, length, nameEnd + 1, quantityPrefix)) return false;
  const quantityStart: u32 = nameEnd + 1 + quantityPrefix.length;
  const quantityEnd: u32 = findByte(value, length, quantityStart, 125);
  if (quantityEnd === 4294967295) return false;
  const quantity: u32 = parseDecimalU32(value, quantityStart, quantityEnd);
  if (quantity === 4294967295 || quantity === 0 || quantity > 99) return false;
  if (!matchesAscii(value, length, quantityEnd, "}}") || quantityEnd + 2 !== length) return false;

  fields[0] = requestStart;
  fields[1] = requestLength;
  fields[2] = nameStart;
  fields[3] = nameLength;
  fields[4] = quantity;
  return true;
}
