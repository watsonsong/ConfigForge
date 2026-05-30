// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

export const SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0" as const;

export interface RuntimeDataReaderLike {
  enterField(name: string): boolean;
  enterIndex(index: number): void;
  exit(): void;
  length(): number;
  asNumber(): number;
  asBool(): boolean;
  asString(): string;
  schemaRevision?(): bigint | number | undefined;
  decodeLimits?(): RuntimeDecodeLimits;
  enterRequiredField?(name: string): void;
  enterOptionalField?(name: string): boolean;
  exitField?(): void;
  readEnumIndex?(variants: readonly string[]): number;
  readRowCount?(): number;
  readListLength?(): number;
  readArrayLength?(): number;
  arrayElementsArePositional?(): boolean;
  beginRecord?(optionalSlotCount: number): void;
  endRecord?(): void;
  readOptionalPresent?(): boolean;
  readNextInt64?(): number;
  readNextFloat64?(): number;
  readNextNumber?(): number;
  readNextBool?(): boolean;
  readNextString?(): string;
  readEnumOrdinal?(): number;
  ensureExhausted?(): void;
}

export interface RuntimeDecodeLimits {
  maxFileBytes: number;
  maxRows: number;
  maxArrayLength: number;
  maxStringBytes: number;
  maxNestingDepth: number;
}

export const DEFAULT_RUNTIME_DECODE_LIMITS: RuntimeDecodeLimits = Object.freeze({
  maxFileBytes: 64 * 1024 * 1024,
  maxRows: 1_000_000,
  maxArrayLength: 1_000_000,
  maxStringBytes: 16 * 1024 * 1024,
  maxNestingDepth: 128,
});

export type RuntimeDecodeLimitsInput = Partial<RuntimeDecodeLimits>;

export function normalizeRuntimeDecodeLimits(
  limits: RuntimeDecodeLimitsInput = {},
): RuntimeDecodeLimits {
  return {
    maxFileBytes: normalizeLimit("maxFileBytes", limits.maxFileBytes, DEFAULT_RUNTIME_DECODE_LIMITS.maxFileBytes),
    maxRows: normalizeLimit("maxRows", limits.maxRows, DEFAULT_RUNTIME_DECODE_LIMITS.maxRows),
    maxArrayLength: normalizeLimit("maxArrayLength", limits.maxArrayLength, DEFAULT_RUNTIME_DECODE_LIMITS.maxArrayLength),
    maxStringBytes: normalizeLimit("maxStringBytes", limits.maxStringBytes, DEFAULT_RUNTIME_DECODE_LIMITS.maxStringBytes),
    maxNestingDepth: normalizeLimit("maxNestingDepth", limits.maxNestingDepth, DEFAULT_RUNTIME_DECODE_LIMITS.maxNestingDepth),
  };
}

export function checkRuntimeDecodeLimit(label: string, actual: number, limit: number): void {
  if (!Number.isSafeInteger(actual) || actual < 0) {
    throw new Error(`${label} must be a non-negative safe integer`);
  }
  if (actual > limit) {
    throw new Error(`${label} ${actual} exceeds decode limit ${limit}`);
  }
}

export interface RuntimeDataLoaderLike {
  open(path: string): RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
}

export type RuntimeReaderDecoder = (
  bytes: Uint8Array,
  path: string,
) => RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;

export class RuntimeValueReader implements RuntimeDataReaderLike {
  private readonly stack: unknown[];
  private readonly limits: RuntimeDecodeLimits;

  constructor(value: unknown, limits: RuntimeDecodeLimitsInput = {}) {
    this.stack = [value];
    this.limits = normalizeRuntimeDecodeLimits(limits);
  }

  enterField(name: string): boolean {
    const value = this.currentValue();
    if (value === null || typeof value !== "object" || Array.isArray(value)) {
      return false;
    }
    const object = value as Record<string, unknown>;
    if (!Object.prototype.hasOwnProperty.call(object, name)) {
      return false;
    }
    const fieldValue = object[name];
    if (fieldValue === null || fieldValue === undefined) {
      return false;
    }
    this.checkNestingDepth(this.stack.length);
    this.stack.push(fieldValue);
    return true;
  }

  enterIndex(index: number): void {
    const value = this.currentValue();
    if (!Array.isArray(value) || index < 0 || index >= value.length) {
      throw new Error(`missing array index ${index}`);
    }
    this.checkNestingDepth(this.stack.length);
    this.stack.push(value[index]);
  }

  exit(): void {
    if (this.stack.length <= 1) {
      throw new Error("runtime reader cursor stack underflow");
    }
    this.stack.pop();
  }

  length(): number {
    const value = this.currentValue();
    if (Array.isArray(value)) {
      return value.length;
    }
    if (value !== null && typeof value === "object") {
      return Object.keys(value).length;
    }
    throw new Error("reader value is not array/object");
  }

  readRowCount(): number {
    const length = this.length();
    checkRuntimeDecodeLimit("row count", length, this.limits.maxRows);
    return length;
  }

  readListLength(): number {
    const length = this.length();
    checkRuntimeDecodeLimit("list length", length, this.limits.maxArrayLength);
    return length;
  }

  readArrayLength(): number {
    return this.readListLength();
  }

  decodeLimits(): RuntimeDecodeLimits {
    return this.limits;
  }

  asNumber(): number {
    let parsed: number;
    const value = this.currentValue();
    if (typeof value === "number") {
      parsed = value;
    } else if (typeof value === "string") {
      if (value.trim().length === 0) {
        throw new Error("expected non-empty number-like string");
      }
      parsed = Number(value);
    } else {
      throw new Error(`expected number-like value, got ${String(value)}`);
    }
    if (!Number.isFinite(parsed)) {
      throw new Error(`expected finite number-like value, got ${String(value)}`);
    }
    return parsed;
  }

  asBool(): boolean {
    const value = this.currentValue();
    if (typeof value === "boolean") {
      return value;
    }
    if (typeof value === "string") {
      if (value === "true" || value === "1") {
        return true;
      }
      if (value === "false" || value === "0") {
        return false;
      }
    }
    throw new Error(`expected bool-like value, got ${String(value)}`);
  }

  asString(): string {
    const value = this.currentValue();
    if (typeof value === "string") {
      checkRuntimeDecodeLimit("string byte length", utf8ByteLength(value), this.limits.maxStringBytes);
      return value;
    }
    throw new Error(`expected string value, got ${String(value)}`);
  }

  private currentValue(): unknown {
    return this.stack[this.stack.length - 1];
  }

  private checkNestingDepth(depth: number): void {
    checkRuntimeDecodeLimit("nesting depth", depth, this.limits.maxNestingDepth);
  }
}

export function readRuntimeRowCount(reader: RuntimeDataReaderLike): number {
  if (!reader.readRowCount) {
    throw new Error("runtime reader does not support row count");
  }
  return reader.readRowCount();
}

export function readRuntimeListLength(reader: RuntimeDataReaderLike): number {
  if (reader.readListLength) {
    return reader.readListLength();
  }
  if (reader.readArrayLength) {
    return reader.readArrayLength();
  }
  throw new Error("runtime reader does not support list length");
}

function normalizeLimit(label: string, value: number | undefined, fallback: number): number {
  const resolved = value ?? fallback;
  if (!Number.isSafeInteger(resolved) || resolved < 0) {
    throw new Error(`${label} must be a non-negative safe integer`);
  }
  return resolved;
}

function utf8ByteLength(value: string): number {
  if (typeof TextEncoder !== "undefined") {
    return new TextEncoder().encode(value).length;
  }
  let bytes = 0;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code <= 0x7f) {
      bytes += 1;
    } else if (code <= 0x7ff) {
      bytes += 2;
    } else if (code >= 0xd800 && code <= 0xdbff && index + 1 < value.length) {
      const next = value.charCodeAt(index + 1);
      if (next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4;
        index += 1;
      } else {
        bytes += 3;
      }
    } else {
      bytes += 3;
    }
  }
  return bytes;
}
