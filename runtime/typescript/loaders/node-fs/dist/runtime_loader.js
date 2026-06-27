"use strict";
// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0
Object.defineProperty(exports, "__esModule", { value: true });
exports.SyncFsRuntimeDataLoader = exports.FsRuntimeDataLoader = exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = void 0;
const runtime_reader_core_1 = require("@config-forge/runtime-reader-core");
Object.defineProperty(exports, "SUPPORTED_CONFIG_FORGE_READER_CONTRACT", { enumerable: true, get: function () { return runtime_reader_core_1.SUPPORTED_CONFIG_FORGE_READER_CONTRACT; } });
const fs = require("fs");
const pathModule = require("path");
class FsRuntimeDataLoader {
    constructor(dataDir, decoder, extension, limits = {}) {
        this.decoder = decoder;
        this.extension = extension;
        this.resolvedDataDir = pathModule.resolve(dataDir);
        this.limits = (0, runtime_reader_core_1.normalizeRuntimeDecodeLimits)(limits);
    }
    async open(path) {
        const fullPath = safeJoin(this.resolvedDataDir, withExtension(path, this.extension));
        const stat = await fs.promises.stat(fullPath);
        (0, runtime_reader_core_1.checkRuntimeDecodeLimit)("runtime file bytes", stat.size, this.limits.maxFileBytes);
        const bytes = await fs.promises.readFile(fullPath);
        (0, runtime_reader_core_1.checkRuntimeDecodeLimit)("runtime file bytes", bytes.byteLength, this.limits.maxFileBytes);
        return this.decoder(bytes, path);
    }
}
exports.FsRuntimeDataLoader = FsRuntimeDataLoader;
class SyncFsRuntimeDataLoader {
    constructor(dataDir, decoder, extension, limits = {}) {
        this.decoder = decoder;
        this.extension = extension;
        this.resolvedDataDir = pathModule.resolve(dataDir);
        this.limits = (0, runtime_reader_core_1.normalizeRuntimeDecodeLimits)(limits);
    }
    open(path) {
        const fullPath = safeJoin(this.resolvedDataDir, withExtension(path, this.extension));
        const stat = fs.statSync(fullPath);
        (0, runtime_reader_core_1.checkRuntimeDecodeLimit)("runtime file bytes", stat.size, this.limits.maxFileBytes);
        const bytes = fs.readFileSync(fullPath);
        (0, runtime_reader_core_1.checkRuntimeDecodeLimit)("runtime file bytes", bytes.byteLength, this.limits.maxFileBytes);
        return this.decoder(bytes, path);
    }
}
exports.SyncFsRuntimeDataLoader = SyncFsRuntimeDataLoader;
function withExtension(logicalPath, extension) {
    if (!extension || /(^|[/\\])[^/\\]*\.[^/\\]+$/.test(logicalPath)) {
        return logicalPath;
    }
    return `${logicalPath}.${extension.replace(/^\./, "")}`;
}
function safeJoin(resolvedRoot, logicalPath) {
    if (pathModule.isAbsolute(logicalPath)) {
        throw runtime_reader_core_1.RuntimeDataError.invalidPath(`runtime data path must be relative: ${logicalPath}`, { logicalPath });
    }
    const resolvedPath = pathModule.resolve(resolvedRoot, logicalPath);
    const relative = pathModule.relative(resolvedRoot, resolvedPath);
    if (relative.startsWith("..") || pathModule.isAbsolute(relative)) {
        throw runtime_reader_core_1.RuntimeDataError.invalidPath(`runtime data path escapes data root: ${logicalPath}`, { logicalPath, resourcePath: resolvedPath });
    }
    return resolvedPath;
}
