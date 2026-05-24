import { RuntimeDataReaderLike, RuntimeReaderDecoder, SUPPORTED_CONFIG_FORGE_READER_CONTRACT } from "@configforge/runtime-reader-core";
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
