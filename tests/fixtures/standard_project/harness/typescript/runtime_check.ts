// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

declare function require(name: string): any;

import { GameData, RuntimeDataLoader } from "./generated";
import { SyncFsRuntimeDataLoader } from "./runtime_loader_node_fs";
import { decodeJsonValue, JsonValueReader } from "./runtime_loader_json";
import { createMessagePackRuntimeDecoder } from "./runtime_loader_messagepack";

const process = require("process");

const dataDir = process.env.DATA_DIR;
if (!dataDir) {
  throw new Error("DATA_DIR should point to exported JSON data");
}

const loader = new SyncFsRuntimeDataLoader(dataDir, decodeJsonValue);
const messagePackLoader: RuntimeDataLoader = new SyncFsRuntimeDataLoader(
  dataDir,
  createMessagePackRuntimeDecoder(() => ({ rows: [] })),
);
void messagePackLoader;

if (!new JsonValueReader("1").asBool() || new JsonValueReader("0").asBool()) {
  throw new Error("numeric bool string parsing failed");
}

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
