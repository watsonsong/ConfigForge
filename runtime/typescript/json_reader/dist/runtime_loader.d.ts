import { RuntimeValueReader, SUPPORTED_CONFIG_FORGE_READER_CONTRACT } from "@config-forge/runtime-reader-core";
export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };
export declare const JsonValueReader: typeof RuntimeValueReader;
export type JsonValueReader = RuntimeValueReader;
export declare function decodeJsonValue(bytes: Uint8Array | string): JsonValueReader;
