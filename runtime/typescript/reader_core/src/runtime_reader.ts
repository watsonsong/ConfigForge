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
  isNull(): boolean;
  schemaRevision?(): bigint | number | undefined;
  isPositional?(): boolean;
  readRowCount?(): number;
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

export interface RuntimeDataLoaderLike {
  open(path: string): RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
}

export type RuntimeReaderDecoder = (
  bytes: Uint8Array,
  path: string,
) => RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;

export class RuntimeValueReader implements RuntimeDataReaderLike {
  private readonly stack: unknown[];

  constructor(value: unknown) {
    this.stack = [value];
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
    this.stack.push(object[name]);
    return true;
  }

  enterIndex(index: number): void {
    const value = this.currentValue();
    if (!Array.isArray(value) || index < 0 || index >= value.length) {
      throw new Error(`missing array index ${index}`);
    }
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
      return value;
    }
    throw new Error(`expected string value, got ${String(value)}`);
  }

  isNull(): boolean {
    const value = this.currentValue();
    return value === null || value === undefined;
  }

  private currentValue(): unknown {
    return this.stack[this.stack.length - 1];
  }
}
