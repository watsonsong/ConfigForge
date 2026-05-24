// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

import {
  RuntimeValueReader,
  SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
} from "@config-forge/runtime-reader-core";

export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };

export type MessagePackDecode = (bytes: Uint8Array) => unknown;

export const MessagePackValueReader = RuntimeValueReader;
export type MessagePackValueReader = RuntimeValueReader;

export function decodeMessagePackValue(
  bytes: Uint8Array,
  decode: MessagePackDecode,
): MessagePackValueReader {
  return new MessagePackValueReader(decode(bytes));
}

export function createMessagePackRuntimeDecoder(
  decode: MessagePackDecode,
): (bytes: Uint8Array, path: string) => MessagePackValueReader {
  return (bytes) => decodeMessagePackValue(bytes, decode);
}
