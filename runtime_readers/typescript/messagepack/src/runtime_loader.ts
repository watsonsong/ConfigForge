import {
  RuntimeDataReaderLike,
  RuntimeValueReader,
  SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
} from "@config-forge/runtime-reader-core";

export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };

export type MessagePackDecode = (bytes: Uint8Array) => unknown;

export class MessagePackValueReader extends RuntimeValueReader implements RuntimeDataReaderLike {}

export function decodeMessagePackValue(
  bytes: Uint8Array,
  decode: MessagePackDecode,
): MessagePackValueReader {
  return new MessagePackValueReader(decode(bytes));
}
