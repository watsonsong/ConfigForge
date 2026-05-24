// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

declare function require(name: string): any;

import {
  RuntimeDataReaderLike,
  RuntimeReaderDecoder,
  SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
} from "@config-forge/runtime-reader-core";

export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };

const fs = require("fs");
const pathModule = require("path");

export class FsRuntimeDataLoader {
  constructor(
    private readonly dataDir: string,
    private readonly decoder: RuntimeReaderDecoder,
  ) {}

  async open(path: string): Promise<RuntimeDataReaderLike> {
    const fullPath = safeJoin(this.dataDir, path);
    const bytes = new Uint8Array(await fs.promises.readFile(fullPath));
    return this.decoder(bytes, path);
  }
}

export class SyncFsRuntimeDataLoader {
  constructor(
    private readonly dataDir: string,
    private readonly decoder: RuntimeReaderDecoder,
  ) {}

  open(path: string): RuntimeDataReaderLike | Promise<RuntimeDataReaderLike> {
    const fullPath = safeJoin(this.dataDir, path);
    return this.decoder(new Uint8Array(fs.readFileSync(fullPath)), path);
  }
}

function safeJoin(root: string, logicalPath: string): string {
  if (pathModule.isAbsolute(logicalPath)) {
    throw new Error(`runtime data path must be relative: ${logicalPath}`);
  }
  const resolvedRoot = pathModule.resolve(root);
  const resolvedPath = pathModule.resolve(resolvedRoot, logicalPath);
  const relative = pathModule.relative(resolvedRoot, resolvedPath);
  if (relative.startsWith("..") || pathModule.isAbsolute(relative)) {
    throw new Error(`runtime data path escapes data root: ${logicalPath}`);
  }
  return resolvedPath;
}
