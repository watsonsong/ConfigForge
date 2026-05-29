// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

import {
  RuntimeDataReaderLike,
  SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
} from "@config-forge/runtime-reader-core";

export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };

const textDecoder = new TextDecoder("utf-8", { fatal: true });

export class BinaryRuntimeDataReader implements RuntimeDataReaderLike {
  private offset = 16;
  private nextRowIndex = 0;
  private readonly revision: bigint;
  private readonly records: Array<{ bitmap: Uint8Array<ArrayBufferLike>; nextOptionalSlot: number }> = [];

  constructor(private readonly bytes: Uint8Array) {
    if (bytes.length < 16) {
      throw new Error("truncated binary runtime data header");
    }
    if (bytes[0] !== 0x43 || bytes[1] !== 0x46 || bytes[2] !== 0x47 || bytes[3] !== 0x42) {
      throw new Error("invalid binary runtime data magic");
    }
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const version = view.getUint16(4, true);
    if (version !== 1) {
      throw new Error(`unsupported binary runtime data version ${version}`);
    }
    const headerSize = view.getUint16(6, true);
    if (headerSize !== 16) {
      throw new Error(`unsupported binary runtime data header size ${headerSize}`);
    }
    this.revision = view.getBigUint64(8, true);
  }

  enterField(_name: string): boolean {
    throw new Error("binary reader does not support named fields");
  }

  enterIndex(index: number): void {
    if (index !== this.nextRowIndex) {
      throw new Error(`binary reader rows must be read sequentially: expected index ${this.nextRowIndex}, got ${index}`);
    }
    this.nextRowIndex++;
  }

  exit(): void {
    throw new Error("binary reader does not support nested named readers");
  }

  length(): number {
    throw new Error("binary reader does not support named array length");
  }

  asNumber(): number {
    return this.readNextNumber();
  }

  asBool(): boolean {
    return this.readNextBool();
  }

  asString(): string {
    return this.readNextString();
  }

  isNull(): boolean {
    return false;
  }

  schemaRevision(): bigint {
    return this.revision;
  }

  enterRequiredField(_name: string): void {
  }

  enterOptionalField(_name: string): boolean {
    return this.readOptionalPresent();
  }

  exitField(): void {
  }

  readEnumIndex(variants: readonly string[]): number {
    const ordinal = this.readEnumOrdinal();
    if (ordinal >= variants.length) {
      throw new Error(`enum ordinal ${ordinal} is out of range`);
    }
    return ordinal;
  }

  readRowCount(): number {
    const rowCount = this.readVaruintAsNumber("row count");
    this.nextRowIndex = 0;
    return rowCount;
  }

  beginRecord(optionalSlotCount: number): void {
    const byteCount = Math.ceil(optionalSlotCount / 8);
    this.records.push({ bitmap: this.readExact(byteCount), nextOptionalSlot: 0 });
  }

  endRecord(): void {
    const record = this.records.pop();
    if (!record) {
      throw new Error("binary reader record stack underflow");
    }
    if (record.nextOptionalSlot > record.bitmap.length * 8) {
      throw new Error("binary reader consumed more optional slots than current record bitmap");
    }
  }

  readOptionalPresent(): boolean {
    const record = this.records[this.records.length - 1];
    if (!record) {
      throw new Error("binary reader is not inside a record");
    }
    const slotIndex = record.nextOptionalSlot++;
    const byte = record.bitmap[Math.floor(slotIndex / 8)];
    if (byte === undefined) {
      throw new Error(`optional slot ${slotIndex} is out of current record bitmap`);
    }
    return (byte & (1 << (slotIndex % 8))) !== 0;
  }

  readNextNumber(): number {
    return this.readNextFloat64();
  }

  readNextInt64(): number {
    const encoded = this.readVaruint();
    const signed = Number((encoded >> 1n) ^ (-(encoded & 1n)));
    if (!Number.isSafeInteger(signed)) {
      throw new Error("int64 value exceeds safe JavaScript integer range");
    }
    return signed;
  }

  readNextFloat64(): number {
    const bytes = this.readExact(8);
    const value = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getFloat64(0, true);
    if (!Number.isFinite(value)) {
      throw new Error("non-finite float in binary runtime data");
    }
    return value;
  }

  readNextBool(): boolean {
    const byte = this.readExact(1)[0];
    if (byte === 0) {
      return false;
    }
    if (byte === 1) {
      return true;
    }
    throw new Error(`invalid bool byte ${byte}`);
  }

  readNextString(): string {
    const len = this.readVaruintAsNumber("string length");
    return textDecoder.decode(this.readExact(len));
  }

  readEnumOrdinal(): number {
    return this.readVaruintAsNumber("enum ordinal");
  }

  ensureExhausted(): void {
    if (this.offset !== this.bytes.length) {
      throw new Error(`trailing bytes in binary runtime data: ${this.bytes.length - this.offset} bytes remaining`);
    }
  }

  private readVaruintAsNumber(label: string): number {
    const value = this.readVaruint();
    const number = Number(value);
    if (!Number.isSafeInteger(number)) {
      throw new Error(`${label} exceeds safe JavaScript integer range`);
    }
    return number;
  }

  private readVaruint(): bigint {
    let value = 0n;
    let shift = 0n;
    for (let index = 0; index < 10; index += 1) {
      const byte = this.readExact(1)[0];
      if (index === 9 && (byte & 0x7f) > 1) {
        throw new Error("varuint exceeds u64");
      }
      value |= BigInt(byte & 0x7f) << shift;
      if ((byte & 0x80) === 0) {
        return value;
      }
      shift += 7n;
    }
    throw new Error("malformed varuint");
  }

  private readExact(len: number): Uint8Array<ArrayBufferLike> {
    const end = this.offset + len;
    if (end > this.bytes.length) {
      throw new Error("truncated binary runtime data");
    }
    const value = this.bytes.subarray(this.offset, end);
    this.offset = end;
    return value;
  }
}

export function decodeBinaryRuntimeData(bytes: Uint8Array): BinaryRuntimeDataReader {
  return new BinaryRuntimeDataReader(bytes);
}
