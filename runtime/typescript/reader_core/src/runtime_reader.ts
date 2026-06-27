// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

export const SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0" as const;

export type RuntimeDataErrorKind =
  | "Io"
  | "Decode"
  | "SchemaMismatch"
  | "TypeMismatch"
  | "NotFound"
  | "Unsupported"
  | "InvalidPath";

export interface RuntimeDataErrorContext {
  path?: string;
  logicalPath?: string;
  resourcePath?: string;
  expected?: string;
  actual?: string;
}

export class RuntimeDataError extends Error {
  readonly kind: RuntimeDataErrorKind;
  readonly code: string;
  readonly context: RuntimeDataErrorContext;

  constructor(kind: RuntimeDataErrorKind, message: string, context: RuntimeDataErrorContext = {}, options?: { cause?: unknown }) {
    super(message);
    this.name = "RuntimeDataError";
    this.kind = kind;
    this.code = runtimeDataErrorCode(kind);
    this.context = context;
    if (options && "cause" in options) {
      (this as Error & { cause?: unknown }).cause = options.cause;
    }
  }

  static io(message: string, cause?: unknown, context: RuntimeDataErrorContext = {}): RuntimeDataError {
    return new RuntimeDataError("Io", message, context, { cause });
  }

  static decode(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError {
    return new RuntimeDataError("Decode", message, context);
  }

  static schemaMismatch(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError {
    return new RuntimeDataError("SchemaMismatch", message, context);
  }

  static typeMismatch(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError {
    return new RuntimeDataError("TypeMismatch", message, context);
  }

  static notFound(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError {
    return new RuntimeDataError("NotFound", message, context);
  }

  static unsupported(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError {
    return new RuntimeDataError("Unsupported", message, context);
  }

  static invalidPath(message: string, context: RuntimeDataErrorContext = {}): RuntimeDataError {
    return new RuntimeDataError("InvalidPath", message, context);
  }
}

export function runtimeDataErrorCode(kind: RuntimeDataErrorKind): string {
  switch (kind) {
    case "Io": return "CF_RT_IO";
    case "Decode": return "CF_RT_DECODE";
    case "SchemaMismatch": return "CF_RT_SCHEMA_MISMATCH";
    case "TypeMismatch": return "CF_RT_TYPE_MISMATCH";
    case "NotFound": return "CF_RT_NOT_FOUND";
    case "Unsupported": return "CF_RT_UNSUPPORTED";
    case "InvalidPath": return "CF_RT_INVALID_PATH";
  }
}

export interface RuntimeDataReaderLike {
  enterRequiredField(name: string): void;
  enterOptionalField(name: string): boolean;
  exitField(): void;
  enterIndex(index: number): void;
  exit(): void;
  readRowCount(): number;
  readListLength(): number;
  readEnumIndex(variants: readonly string[]): number;
  arrayElementsArePositional(): boolean;
  beginRecord(optionalSlotCount: number): void;
  endRecord(): void;
  asInt64(): number;
  asFloat64(): number;
  asBool(): boolean;
  asString(): string;
  schemaRevision?(): bigint | number | undefined;
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
    throw RuntimeDataError.decode(`${label} must be a non-negative safe integer`, { expected: "non-negative safe integer", actual: String(actual) });
  }
  if (actual > limit) {
    throw RuntimeDataError.decode(`${label} ${actual} exceeds decode limit ${limit}`, { expected: `<= ${limit}`, actual: String(actual) });
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
      throw RuntimeDataError.notFound(`missing array index ${index}`, { expected: "array index", actual: runtimeTypeOf(value) });
    }
    this.checkNestingDepth(this.stack.length);
    this.stack.push(value[index]);
  }

  exit(): void {
    if (this.stack.length <= 1) {
      throw RuntimeDataError.decode("runtime reader cursor stack underflow");
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
    throw RuntimeDataError.typeMismatch("reader value is not array/object", { expected: "array/object", actual: runtimeTypeOf(value) });
  }

  enterRequiredField(name: string): void {
    if (!this.enterField(name)) {
      throw RuntimeDataError.notFound(`missing required field ${name}`, { expected: `field ${name}`, actual: "missing/null" });
    }
  }

  enterOptionalField(name: string): boolean {
    return this.enterField(name);
  }

  exitField(): void {
    this.exit();
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

  readEnumIndex(variants: readonly string[]): number {
    const value = this.asString();
    const index = variants.indexOf(value);
    if (index < 0) {
      throw RuntimeDataError.decode(`unknown enum value ${value}`, { expected: variants.join(" | "), actual: value });
    }
    return index;
  }

  arrayElementsArePositional(): boolean {
    return false;
  }

  beginRecord(_optionalSlotCount: number): void {
  }

  endRecord(): void {
  }

  asInt64(): number {
    let parsed: number;
    const value = this.currentValue();
    if (typeof value === "number") {
      parsed = value;
    } else if (typeof value === "string") {
      if (value.trim().length === 0) {
        throw RuntimeDataError.typeMismatch("expected non-empty number-like string", { expected: "number-like string", actual: "empty string" });
      }
      parsed = Number(value);
    } else {
      throw RuntimeDataError.typeMismatch(`expected number-like value, got ${String(value)}`, { expected: "number-like value", actual: runtimeTypeOf(value) });
    }
    if (!Number.isFinite(parsed)) {
      throw RuntimeDataError.typeMismatch(`expected finite number-like value, got ${String(value)}`, { expected: "finite number-like value", actual: String(value) });
    }
    if (!Number.isSafeInteger(parsed)) {
      throw RuntimeDataError.decode(`expected safe integer-like value, got ${String(value)}`, { expected: "safe integer-like value", actual: String(value) });
    }
    return parsed;
  }

  asFloat64(): number {
    return this.asFiniteNumber();
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
    throw RuntimeDataError.decode(`expected bool-like value, got ${String(value)}`, { expected: "bool-like value", actual: String(value) });
  }

  asString(): string {
    const value = this.currentValue();
    if (typeof value === "string") {
      checkRuntimeDecodeLimit("string byte length", utf8ByteLength(value), this.limits.maxStringBytes);
      return value;
    }
    throw RuntimeDataError.typeMismatch(`expected string value, got ${String(value)}`, { expected: "string", actual: runtimeTypeOf(value) });
  }

  ensureExhausted(): void {
  }

  private asFiniteNumber(): number {
    let parsed: number;
    const value = this.currentValue();
    if (typeof value === "number") {
      parsed = value;
    } else if (typeof value === "string") {
      if (value.trim().length === 0) {
        throw RuntimeDataError.typeMismatch("expected non-empty number-like string", { expected: "number-like string", actual: "empty string" });
      }
      parsed = Number(value);
    } else {
      throw RuntimeDataError.typeMismatch(`expected number-like value, got ${String(value)}`, { expected: "number-like value", actual: runtimeTypeOf(value) });
    }
    if (!Number.isFinite(parsed)) {
      throw RuntimeDataError.typeMismatch(`expected finite number-like value, got ${String(value)}`, { expected: "finite number-like value", actual: String(value) });
    }
    return parsed;
  }

  private currentValue(): unknown {
    return this.stack[this.stack.length - 1];
  }

  private checkNestingDepth(depth: number): void {
    checkRuntimeDecodeLimit("nesting depth", depth, this.limits.maxNestingDepth);
  }
}

function normalizeLimit(label: string, value: number | undefined, fallback: number): number {
  const resolved = value ?? fallback;
  if (!Number.isSafeInteger(resolved) || resolved < 0) {
    throw RuntimeDataError.decode(`${label} must be a non-negative safe integer`, { expected: "non-negative safe integer", actual: String(resolved) });
  }
  return resolved;
}

function runtimeTypeOf(value: unknown): string {
  if (value === null) {
    return "null";
  }
  if (Array.isArray(value)) {
    return "array";
  }
  return typeof value;
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
