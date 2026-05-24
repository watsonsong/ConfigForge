import { RuntimeDataLoaderLike, RuntimeDataReaderLike, RuntimeReaderDecoder, SUPPORTED_CONFIG_FORGE_READER_CONTRACT } from "@config-forge/runtime-reader-core";
export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };
export type SyncRuntimeReaderDecoder = (bytes: Uint8Array, path: string) => RuntimeDataReaderLike;
export declare class FsRuntimeDataLoader implements RuntimeDataLoaderLike {
    private readonly decoder;
    private readonly resolvedDataDir;
    constructor(dataDir: string, decoder: RuntimeReaderDecoder);
    open(path: string): Promise<RuntimeDataReaderLike>;
}
export declare class SyncFsRuntimeDataLoader implements RuntimeDataLoaderLike {
    private readonly decoder;
    private readonly resolvedDataDir;
    constructor(dataDir: string, decoder: SyncRuntimeReaderDecoder);
    open(path: string): RuntimeDataReaderLike;
}
