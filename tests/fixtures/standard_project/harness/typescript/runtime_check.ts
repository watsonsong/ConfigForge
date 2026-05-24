declare function require(name: string): any;

import { GameData, RuntimeDataLoader, RuntimeDataReader } from "./generated";

const fs = require("fs");
const pathModule = require("path");
const process = require("process");

const dataDir = process.env.DATA_DIR;
if (!dataDir) {
  throw new Error("DATA_DIR should point to exported JSON data");
}

class ValueReader implements RuntimeDataReader {
  constructor(private readonly value: any) {}

  field(name: string): RuntimeDataReader | undefined {
    if (this.value === null || typeof this.value !== "object" || Array.isArray(this.value)) {
      return undefined;
    }
    return Object.prototype.hasOwnProperty.call(this.value, name)
      ? new ValueReader(this.value[name])
      : undefined;
  }

  index(index: number): RuntimeDataReader {
    if (!Array.isArray(this.value) || index < 0 || index >= this.value.length) {
      throw new Error(`missing array index ${index}`);
    }
    return new ValueReader(this.value[index]);
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
      return this.value === "true";
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

const loader: RuntimeDataLoader = {
  open(path: string) {
    const fullPath = pathModule.join(dataDir, path);
    return new ValueReader(JSON.parse(fs.readFileSync(fullPath, "utf8")));
  },
};

async function main(): Promise<void> {
  const data = await GameData.load(loader);

  const item = data.items.getById(2001);
  if (!item || item.name !== "Iron Ore" || item.stack?.maxStack !== 999) {
    throw new Error("single-field item index query failed");
  }
  if (data.items.getById(404)) {
    throw new Error("missing item index query should return undefined");
  }

  const classLevel = data.classLevels.getByClassLevel(1, 2);
  if (!classLevel || classLevel.attributes.maxHealth !== 145) {
    throw new Error("compound class level index query failed");
  }

  if (data.gameRules.maxPlayerLevel !== 60 || !data.gameRules.pvpEnabled) {
    throw new Error("object table load failed");
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
