import { RuntimeDataReaderLike, RuntimeValueReader, SUPPORTED_CONFIG_FORGE_READER_CONTRACT } from "@config-forge/runtime-reader-core";
export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };
export declare class JsonValueReader extends RuntimeValueReader implements RuntimeDataReaderLike {
}
export declare function decodeJsonValue(bytes: Uint8Array | string): JsonValueReader;
