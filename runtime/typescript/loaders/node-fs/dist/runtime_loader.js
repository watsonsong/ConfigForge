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
    constructor(dataDir, decoder) {
        this.decoder = decoder;
        this.resolvedDataDir = pathModule.resolve(dataDir);
    }
    async open(path) {
        const fullPath = safeJoin(this.resolvedDataDir, path);
        const bytes = await fs.promises.readFile(fullPath);
        return this.decoder(bytes, path);
    }
}
exports.FsRuntimeDataLoader = FsRuntimeDataLoader;
class SyncFsRuntimeDataLoader {
    constructor(dataDir, decoder) {
        this.decoder = decoder;
        this.resolvedDataDir = pathModule.resolve(dataDir);
    }
    open(path) {
        const fullPath = safeJoin(this.resolvedDataDir, path);
        return this.decoder(fs.readFileSync(fullPath), path);
    }
}
exports.SyncFsRuntimeDataLoader = SyncFsRuntimeDataLoader;
function safeJoin(resolvedRoot, logicalPath) {
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
