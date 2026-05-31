// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

declare function require(name: string): any;

import { GameData, RuntimeDataLoader } from "./generated";
import { SyncFsRuntimeDataLoader } from "./runtime_loader_node_fs";
import { decodeBinaryRuntimeData } from "./runtime_loader_binary";
import { decodeJsonValue, JsonValueReader } from "./runtime_loader_json";
import { createMessagePackRuntimeDecoder } from "./runtime_loader_messagepack";

const process = require("process");

const dataDir = process.env.DATA_DIR;
const binaryDataDir = process.env.BIN_DATA_DIR;
if (!dataDir && !binaryDataDir) {
  throw new Error("DATA_DIR or BIN_DATA_DIR should point to exported runtime data");
}

const binaryLoader = binaryDataDir
  ? new SyncFsRuntimeDataLoader(binaryDataDir, decodeBinaryRuntimeData, "bin")
  : undefined;
const loader: RuntimeDataLoader | undefined = dataDir
  ? new SyncFsRuntimeDataLoader(dataDir, decodeJsonValue, "json")
  : undefined;
if (dataDir) {
  const messagePackLoader: RuntimeDataLoader = new SyncFsRuntimeDataLoader(
    dataDir,
    createMessagePackRuntimeDecoder(() => ({ rows: [] })),
    "msgpack",
  );
  void messagePackLoader;
}

if (!new JsonValueReader("1").asBool() || new JsonValueReader("0").asBool()) {
  throw new Error("numeric bool string parsing failed");
}

async function main(): Promise<void> {
  if (binaryLoader) {
    const binaryData = await GameData.load(binaryLoader);
    const binaryItem = binaryData.items.getById(2001);
    if (!binaryItem || binaryItem.name !== "Iron Ore" || binaryItem.stack?.maxStack !== 999) {
      throw new Error("binary single-field item index query failed");
    }
    const binaryClassLevel = binaryData.classLevels.getByClassLevel(1, 2);
    if (!binaryClassLevel || binaryClassLevel.attributes.maxHealth !== 145) {
      throw new Error("binary compound class level index query failed");
    }
    if (binaryData.gameRules.maxPlayerLevel !== 60 || !binaryData.gameRules.pvpEnabled) {
      throw new Error("binary object table load failed");
    }
    if (binaryData.gameRules.defaultClassId !== 1 || binaryData.gameRules.starterItemIds.join(",") !== "2001,2002") {
      throw new Error("binary object table array field load failed");
    }
    if (binaryData.gameRules.defaultClass(binaryData).classId !== 1) {
      throw new Error("binary object table single relation failed");
    }
    const binaryStarterItems = binaryData.gameRules.starterItems(binaryData);
    if (binaryStarterItems.length !== 2 || binaryStarterItems[0]?.id !== 2001 || binaryStarterItems[1]?.id !== 2002) {
      throw new Error("binary object table list relation failed");
    }
    binaryData.linkAll();
    if (binaryData.gameRules.defaultClass(binaryData).classId !== 1) {
      throw new Error("binary link all relation cache failed");
    }
    return;
  }

  if (!loader) {
    throw new Error("DATA_DIR should point to exported JSON data");
  }

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
  if (data.gameRules.defaultClassId !== 1 || data.gameRules.starterItemIds.join(",") !== "2001,2002") {
    throw new Error("object table array field load failed");
  }
  if (data.gameRules.defaultClass(data).classId !== 1) {
    throw new Error("object table single relation failed");
  }
  const starterItems = data.gameRules.starterItems(data);
  if (starterItems.length !== 2 || starterItems[0]?.id !== 2001 || starterItems[1]?.id !== 2002) {
    throw new Error("object table list relation failed");
  }
  data.linkAll();
  if (data.gameRules.defaultClass(data).classId !== 1) {
    throw new Error("link all relation cache failed");
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
