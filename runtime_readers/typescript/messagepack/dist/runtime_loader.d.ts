import { RuntimeDataReaderLike, RuntimeValueReader, SUPPORTED_CONFIG_FORGE_READER_CONTRACT } from "@config-forge/runtime-reader-core";
export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };
export type MessagePackDecode = (bytes: Uint8Array) => unknown;
export declare class MessagePackValueReader extends RuntimeValueReader implements RuntimeDataReaderLike {
}
export declare function decodeMessagePackValue(bytes: Uint8Array, decode: MessagePackDecode): MessagePackValueReader;
