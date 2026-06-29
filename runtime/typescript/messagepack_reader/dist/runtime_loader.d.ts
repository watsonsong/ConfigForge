import { RuntimeDecodeLimitsInput, RuntimeValueReader, SUPPORTED_CONFIG_FORGE_READER_CONTRACT } from "@config-forge/runtime-reader-core";
export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };
export type MessagePackDecode = (bytes: Uint8Array) => unknown;
export declare const MessagePackValueReader: typeof RuntimeValueReader;
export type MessagePackValueReader = RuntimeValueReader;
export declare function decodeMessagePackValue(bytes: Uint8Array, limits?: RuntimeDecodeLimitsInput): MessagePackValueReader;
export declare function decodeMessagePackValue(bytes: Uint8Array, decode: MessagePackDecode, limits?: RuntimeDecodeLimitsInput): MessagePackValueReader;
export declare function createMessagePackRuntimeDecoder(limits?: RuntimeDecodeLimitsInput): (bytes: Uint8Array, path: string) => MessagePackValueReader;
export declare function createMessagePackRuntimeDecoder(decode: MessagePackDecode, limits?: RuntimeDecodeLimitsInput): (bytes: Uint8Array, path: string) => MessagePackValueReader;
