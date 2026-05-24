declare function require(name: string): any;

const fs = require("fs");
const pathModule = require("path");

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
  open(path: string): RuntimeDataReaderLike;
}

export class JsonValueReader implements RuntimeDataReaderLike {
  constructor(private readonly value: unknown) {}

  field(name: string): RuntimeDataReaderLike | undefined {
    if (this.value === null || typeof this.value !== "object" || Array.isArray(this.value)) {
      return undefined;
    }
    const object = this.value as Record<string, unknown>;
    return Object.prototype.hasOwnProperty.call(object, name)
      ? new JsonValueReader(object[name])
      : undefined;
  }

  index(index: number): RuntimeDataReaderLike {
    if (!Array.isArray(this.value) || index < 0 || index >= this.value.length) {
      throw new Error(`missing array index ${index}`);
    }
    return new JsonValueReader(this.value[index]);
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
      if (this.value === "true") {
        return true;
      }
      if (this.value === "false") {
        return false;
      }
      if (this.value === "1") {
        return true;
      }
      if (this.value === "0") {
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

export class JsonFsRuntimeDataLoader implements RuntimeDataLoaderLike {
  constructor(private readonly dataDir: string) {}

  open(path: string): RuntimeDataReaderLike {
    const fullPath = pathModule.join(this.dataDir, path);
    return new JsonValueReader(JSON.parse(fs.readFileSync(fullPath, "utf8")));
  }
}
