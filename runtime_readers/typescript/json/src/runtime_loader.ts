import {
  RuntimeDataReaderLike,
  RuntimeValueReader,
  SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
} from "@configforge/runtime-reader-core";

export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };

export class JsonValueReader extends RuntimeValueReader implements RuntimeDataReaderLike {}

export function decodeJsonValue(bytes: Uint8Array | string): JsonValueReader {
  const text = typeof bytes === "string" ? bytes : new TextDecoder().decode(bytes);
  return new JsonValueReader(JSON.parse(text));
}
