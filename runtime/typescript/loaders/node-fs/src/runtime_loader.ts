// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

declare function require(name: string): any;

import {
  checkRuntimeDecodeLimit,
  RuntimeDecodeLimits,
  RuntimeDecodeLimitsInput,
  RuntimeDataError,
  RuntimeDataLoaderLike,
  RuntimeDataReaderLike,
  RuntimeReaderDecoder,
  normalizeRuntimeDecodeLimits,
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
  private readonly limits: RuntimeDecodeLimits;

  constructor(
    dataDir: string,
    private readonly decoder: RuntimeReaderDecoder,
    private readonly extension?: string,
    limits: RuntimeDecodeLimitsInput = {},
  ) {
    this.resolvedDataDir = pathModule.resolve(dataDir);
    this.limits = normalizeRuntimeDecodeLimits(limits);
  }

  async open(path: string): Promise<RuntimeDataReaderLike> {
    const fullPath = safeJoin(this.resolvedDataDir, withExtension(path, this.extension));
    const stat = await fs.promises.stat(fullPath);
    checkRuntimeDecodeLimit("runtime file bytes", stat.size, this.limits.maxFileBytes);
    const bytes = await fs.promises.readFile(fullPath);
    checkRuntimeDecodeLimit("runtime file bytes", bytes.byteLength, this.limits.maxFileBytes);
    return this.decoder(bytes, path);
  }
}

export class SyncFsRuntimeDataLoader implements RuntimeDataLoaderLike {
  private readonly resolvedDataDir: string;
  private readonly limits: RuntimeDecodeLimits;

  constructor(
    dataDir: string,
    private readonly decoder: SyncRuntimeReaderDecoder,
    private readonly extension?: string,
    limits: RuntimeDecodeLimitsInput = {},
  ) {
    this.resolvedDataDir = pathModule.resolve(dataDir);
    this.limits = normalizeRuntimeDecodeLimits(limits);
  }

  open(path: string): RuntimeDataReaderLike {
    const fullPath = safeJoin(this.resolvedDataDir, withExtension(path, this.extension));
    const stat = fs.statSync(fullPath);
    checkRuntimeDecodeLimit("runtime file bytes", stat.size, this.limits.maxFileBytes);
    const bytes = fs.readFileSync(fullPath);
    checkRuntimeDecodeLimit("runtime file bytes", bytes.byteLength, this.limits.maxFileBytes);
    return this.decoder(bytes, path);
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
    throw RuntimeDataError.invalidPath(`runtime data path must be relative: ${logicalPath}`, { logicalPath });
  }
  const resolvedPath = pathModule.resolve(resolvedRoot, logicalPath);
  const relative = pathModule.relative(resolvedRoot, resolvedPath);
  if (relative.startsWith("..") || pathModule.isAbsolute(relative)) {
    throw RuntimeDataError.invalidPath(`runtime data path escapes data root: ${logicalPath}`, { logicalPath, resourcePath: resolvedPath });
  }
  return resolvedPath;
}
