// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

declare function require(name: string): any;

import {
  RuntimeDataLoaderLike,
  RuntimeDataReaderLike,
  RuntimeReaderDecoder,
  SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
} from "@config-forge/runtime-reader-core";

export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };

const fs = require("fs");
const pathModule = require("path");

export type SyncRuntimeReaderDecoder = (
  bytes: Uint8Array,
  path: string,
) => RuntimeDataReaderLike;

export class FsRuntimeDataLoader implements RuntimeDataLoaderLike {
  private readonly resolvedDataDir: string;

  constructor(
    dataDir: string,
    private readonly decoder: RuntimeReaderDecoder,
    private readonly extension?: string,
  ) {
    this.resolvedDataDir = pathModule.resolve(dataDir);
  }

  async open(path: string): Promise<RuntimeDataReaderLike> {
    const fullPath = safeJoin(this.resolvedDataDir, withExtension(path, this.extension));
    const bytes = await fs.promises.readFile(fullPath);
    return this.decoder(bytes, path);
  }
}

export class SyncFsRuntimeDataLoader implements RuntimeDataLoaderLike {
  private readonly resolvedDataDir: string;

  constructor(
    dataDir: string,
    private readonly decoder: SyncRuntimeReaderDecoder,
    private readonly extension?: string,
  ) {
    this.resolvedDataDir = pathModule.resolve(dataDir);
  }

  open(path: string): RuntimeDataReaderLike {
    const fullPath = safeJoin(this.resolvedDataDir, withExtension(path, this.extension));
    return this.decoder(fs.readFileSync(fullPath), path);
  }
}

function withExtension(logicalPath: string, extension?: string): string {
  if (!extension || /(^|[/\\])[^/\\]*\.[^/\\]+$/.test(logicalPath)) {
    return logicalPath;
  }
  return `${logicalPath}.${extension.replace(/^\./, "")}`;
}

function safeJoin(resolvedRoot: string, logicalPath: string): string {
  if (pathModule.isAbsolute(logicalPath)) {
    throw new Error(`runtime data path must be relative: ${logicalPath}`);
  }
  const resolvedPath = pathModule.resolve(resolvedRoot, logicalPath);
  const relative = pathModule.relative(resolvedRoot, resolvedPath);
  if (relative.startsWith("..") || pathModule.isAbsolute(relative)) {
    throw new Error(`runtime data path escapes data root: ${logicalPath}`);
  }
  return resolvedPath;
}
