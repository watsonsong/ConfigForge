// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

import {
  RuntimeValueReader,
  SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
} from "@config-forge/runtime-reader-core";

export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };

export const JsonValueReader = RuntimeValueReader;
export type JsonValueReader = RuntimeValueReader;

const utf8Decoder = new TextDecoder();

export function decodeJsonValue(bytes: Uint8Array | string): JsonValueReader {
  const text = typeof bytes === "string" ? bytes : utf8Decoder.decode(bytes);
  return new JsonValueReader(JSON.parse(text));
}
