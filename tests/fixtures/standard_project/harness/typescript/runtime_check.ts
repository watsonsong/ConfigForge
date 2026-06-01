// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

declare function require(name: string): any;

import {
  classesDescriptionText,
  classesNameText,
  GameData,
  itemsNameText,
  LazyGameData,
  RuntimeDataLoader,
} from "./generated";
import { setGlobalTextCatalog } from "./i18n";
import { SyncFsRuntimeDataLoader } from "./runtime_loader_node_fs";
import { decodeBinaryRuntimeData } from "./runtime_loader_binary";
import { decodeJsonValue, JsonValueReader } from "./runtime_loader_json";
import { createMessagePackRuntimeDecoder } from "./runtime_loader_messagepack";

const process = require("process");
const fs = require("fs");

const dataDir = process.env.DATA_DIR;
const binaryDataDir = process.env.BIN_DATA_DIR;
const i18nMo = process.env.I18N_MO;
if (!dataDir && !binaryDataDir) {
  throw new Error("DATA_DIR or BIN_DATA_DIR should point to exported runtime data");
}
if (!i18nMo) {
  throw new Error("I18N_MO should point to the compiled gettext MO catalog");
}

function loadMoCatalog(path: string): Map<string, string> {
  const data = fs.readFileSync(path);
  if (data.readUInt32LE(0) !== 0x950412de) {
    throw new Error("unsupported MO file magic");
  }
  const count = data.readUInt32LE(8);
  const originalTableOffset = data.readUInt32LE(12);
  const translatedTableOffset = data.readUInt32LE(16);
  const catalog = new Map<string, string>();
  for (let index = 0; index < count; index++) {
    const originalLength = data.readUInt32LE(originalTableOffset + index * 8);
    const originalOffset = data.readUInt32LE(originalTableOffset + index * 8 + 4);
    const translatedLength = data.readUInt32LE(translatedTableOffset + index * 8);
    const translatedOffset = data.readUInt32LE(translatedTableOffset + index * 8 + 4);
    const original = data.toString("utf8", originalOffset, originalOffset + originalLength);
    const translated = data.toString("utf8", translatedOffset, translatedOffset + translatedLength);
    catalog.set(original, translated);
  }
  return catalog;
}

const moCatalog = loadMoCatalog(i18nMo);
setGlobalTextCatalog(moCatalog);

function assertTranslatedText(data: GameData): void {
  const item = data.items.getById(2001);
  if (!item || itemsNameText(item) !== "铁矿石") {
    throw new Error("item text translation failed");
  }
  const warrior = data.classes.getById(1);
  if (!warrior || classesNameText(warrior) !== "战士") {
    throw new Error("class name translation failed");
  }
  if (warrior && classesDescriptionText(warrior) !== "耐久的近战战士。") {
    throw new Error("class description translation failed");
  }
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
    if (binaryData.characterSurnames.length !== 4 || binaryData.characterSurnames.rowAt(0)?.value !== "Stone") {
      throw new Error("binary source-order surname table failed");
    }
    if (binaryData.characterSurnames.getByValue("Frost")?.value !== "Frost") {
      throw new Error("binary indexed source-order surname table failed");
    }
    if (binaryData.characterMaleGivenNames.rows()[1]?.value !== "Borin" || binaryData.characterFemaleGivenNames.rows()[2]?.value !== "Celeste") {
      throw new Error("binary source-order given name table failed");
    }
    assertTranslatedText(binaryData);

    const lazyBinaryData = GameData.loadLazy(binaryLoader);
    const lazyBinaryItem = await lazyBinaryData.items().then((items) => items.getById(2001));
    if (!lazyBinaryItem || lazyBinaryItem.id !== 2001) {
      throw new Error("binary lazy item table load failed");
    }
    const lazyBinaryRules = await lazyBinaryData.gameRules();
    if (lazyBinaryRules.defaultClassId !== 1) {
      throw new Error("binary lazy object table load failed");
    }
    await lazyBinaryData.linkAll();

    const syncBinaryData = GameData.loadSync(binaryLoader);
    if (syncBinaryData.items.getById(2001)?.id !== 2001) {
      throw new Error("binary sync eager item table load failed");
    }
    const syncLazyBinaryData = GameData.loadLazy(binaryLoader);
    if (syncLazyBinaryData.itemsSync().getById(2001)?.id !== 2001) {
      throw new Error("binary sync lazy item table load failed");
    }
    if (syncLazyBinaryData.gameRulesSync().defaultClassId !== 1) {
      throw new Error("binary sync lazy object table load failed");
    }
    syncLazyBinaryData.linkAllSync();
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
  if (data.characterSurnames.length !== 4 || data.characterSurnames.rowAt(0)?.value !== "Stone") {
    throw new Error("source-order surname table failed");
  }
  if (data.characterSurnames.getByValue("Frost")?.value !== "Frost") {
    throw new Error("indexed source-order surname table failed");
  }
  if (data.characterMaleGivenNames.rows()[1]?.value !== "Borin" || data.characterFemaleGivenNames.rows()[2]?.value !== "Celeste") {
    throw new Error("source-order given name table failed");
  }
  assertTranslatedText(data);

  const lazyData = new LazyGameData(loader);
  const lazyItems = await lazyData.items();
  const lazyItem = lazyItems.getById(2001);
  if (!lazyItem || lazyItem.name !== "Iron Ore") {
    throw new Error("lazy item table load failed");
  }
  const lazyRules = await lazyData.gameRules();
  if (lazyRules.defaultClassId !== 1 || lazyRules.starterItemIds.join(",") !== "2001,2002") {
    throw new Error("lazy object table load failed");
  }
  await lazyData.linkAll();

  const syncData = GameData.loadSync(loader);
  if (syncData.items.getById(2001)?.name !== "Iron Ore") {
    throw new Error("sync eager item table load failed");
  }
  const syncLazyData = new LazyGameData(loader);
  if (syncLazyData.itemsSync().getById(2001)?.name !== "Iron Ore") {
    throw new Error("sync lazy item table load failed");
  }
  if (syncLazyData.gameRulesSync().defaultClassId !== 1) {
    throw new Error("sync lazy object table load failed");
  }
  syncLazyData.linkAllSync();
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
