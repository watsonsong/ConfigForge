// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

import { RuntimeDataReaderLike, RuntimeReaderDecoder, SUPPORTED_CONFIG_FORGE_READER_CONTRACT } from "@config-forge/runtime-reader-core";
export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };
export declare class FsRuntimeDataLoader {
    private readonly dataDir;
    private readonly decoder;
    constructor(dataDir: string, decoder: RuntimeReaderDecoder);
    open(path: string): Promise<RuntimeDataReaderLike>;
}
export declare class SyncFsRuntimeDataLoader {
    private readonly dataDir;
    private readonly decoder;
    constructor(dataDir: string, decoder: RuntimeReaderDecoder);
    open(path: string): RuntimeDataReaderLike | Promise<RuntimeDataReaderLike>;
}
