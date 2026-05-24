declare function require(name: string): any;

import { GameData, RuntimeDataLoader } from "./generated";

const fs = require("fs");
const pathModule = require("path");
const process = require("process");

const dataDir = process.env.DATA_DIR;
if (!dataDir) {
  throw new Error("DATA_DIR should point to exported JSON data");
}

function readPayload(path: string): any {
  const fullPath = pathModule.join(dataDir, path);
  return JSON.parse(fs.readFileSync(fullPath, "utf8"));
}

function rows(path: string): any[] {
  const payload = readPayload(path);
  if (!Array.isArray(payload.rows)) {
    throw new Error(`collection payload ${path} should contain rows`);
  }
  return payload.rows;
}

function numberValue(value: unknown): number {
  if (typeof value === "number") {
    return value;
  }
  if (typeof value === "string") {
    return Number(value);
  }
  throw new Error(`expected number-like value, got ${String(value)}`);
}

function boolValue(value: unknown): boolean {
  if (typeof value === "boolean") {
    return value;
  }
  if (typeof value === "string") {
    return value === "true";
  }
  throw new Error(`expected bool-like value, got ${String(value)}`);
}

function attributeSet(value: any) {
  return {
    maxHealth: numberValue(value.max_health),
    maxStamina: numberValue(value.max_stamina),
    attack: numberValue(value.attack),
    defence: numberValue(value.defence),
  };
}

const loader: RuntimeDataLoader = {
  loadItemsRows(path: string) {
    return rows(path).map((row) => ({
      id: numberValue(row.id),
      name: String(row.name),
      price: numberValue(row.price),
      stack: row.stack ? { maxStack: numberValue(row.stack.max_stack) } : undefined,
      attributes: row.attributes ? attributeSet(row.attributes) : undefined,
      skill: row.skill ? { skillAsset: String(row.skill.skill_asset) } : undefined,
    }));
  },

  loadClassesRows(path: string) {
    return rows(path).map((row) => ({
      classId: numberValue(row.class_id),
      name: String(row.name),
      description: String(row.description),
      icon: String(row.icon),
    }));
  },

  loadClassLevelsRows(path: string) {
    return rows(path).map((row) => ({
      classId: numberValue(row.class_id),
      level: numberValue(row.level),
      attributes: attributeSet(row.attributes),
    }));
  },

  loadGameRules(path: string) {
    const payload = readPayload(path);
    return {
      maxPlayerLevel: numberValue(payload.max_player_level),
      starterGold: numberValue(payload.starter_gold),
      staminaCap: numberValue(payload.stamina_cap),
      dailyResetHour: numberValue(payload.daily_reset_hour),
      pvpEnabled: boolValue(payload.pvp_enabled),
    };
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
