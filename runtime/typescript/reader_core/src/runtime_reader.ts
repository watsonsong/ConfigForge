// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

export const SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0" as const;

export interface RuntimeDataReaderLike {
  field(name: string): RuntimeDataReaderLike | undefined;
  index(index: number): RuntimeDataReaderLike;
  length(): number;
  asNumber(): number;
  asBool(): boolean;
  asString(): string;
  isNull(): boolean;
}

export interface RuntimeDataLoaderLike {
  open(path: string): RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
}

export type RuntimeReaderDecoder = (
  bytes: Uint8Array,
  path: string,
) => RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;

export class RuntimeValueReader implements RuntimeDataReaderLike {
  constructor(private readonly value: unknown) {}

  field(name: string): RuntimeDataReaderLike | undefined {
    if (this.value === null || typeof this.value !== "object" || Array.isArray(this.value)) {
      return undefined;
    }
    const object = this.value as Record<string, unknown>;
    return Object.prototype.hasOwnProperty.call(object, name)
      ? new RuntimeValueReader(object[name])
      : undefined;
  }

  index(index: number): RuntimeDataReaderLike {
    if (!Array.isArray(this.value) || index < 0 || index >= this.value.length) {
      throw new Error(`missing array index ${index}`);
    }
    return new RuntimeValueReader(this.value[index]);
  }

  length(): number {
    if (Array.isArray(this.value)) {
      return this.value.length;
    }
    if (this.value !== null && typeof this.value === "object") {
      return Object.keys(this.value).length;
    }
    throw new Error("reader value is not array/object");
  }

  asNumber(): number {
    let parsed: number;
    if (typeof this.value === "number") {
      parsed = this.value;
    } else if (typeof this.value === "string") {
      if (this.value.trim().length === 0) {
        throw new Error("expected non-empty number-like string");
      }
      parsed = Number(this.value);
    } else {
      throw new Error(`expected number-like value, got ${String(this.value)}`);
    }
    if (!Number.isFinite(parsed)) {
      throw new Error(`expected finite number-like value, got ${String(this.value)}`);
    }
    return parsed;
  }

  asBool(): boolean {
    if (typeof this.value === "boolean") {
      return this.value;
    }
    if (typeof this.value === "string") {
      if (this.value === "true" || this.value === "1") {
        return true;
      }
      if (this.value === "false" || this.value === "0") {
        return false;
      }
    }
    throw new Error(`expected bool-like value, got ${String(this.value)}`);
  }

  asString(): string {
    if (typeof this.value === "string") {
      return this.value;
    }
    throw new Error(`expected string value, got ${String(this.value)}`);
  }

  isNull(): boolean {
    return this.value === null || this.value === undefined;
  }
}
