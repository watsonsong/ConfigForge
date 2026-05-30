// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

import {
  checkRuntimeDecodeLimit,
  RuntimeDecodeLimitsInput,
  RuntimeValueReader,
  normalizeRuntimeDecodeLimits,
  SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
} from "@config-forge/runtime-reader-core";

export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };

export type MessagePackDecode = (bytes: Uint8Array) => unknown;

export const MessagePackValueReader = RuntimeValueReader;
export type MessagePackValueReader = RuntimeValueReader;

export function decodeMessagePackValue(
  bytes: Uint8Array,
  decode: MessagePackDecode,
  limits: RuntimeDecodeLimitsInput = {},
): MessagePackValueReader {
  const normalizedLimits = normalizeRuntimeDecodeLimits(limits);
  checkRuntimeDecodeLimit("runtime file bytes", bytes.byteLength, normalizedLimits.maxFileBytes);
  return new MessagePackValueReader(decode(bytes), normalizedLimits);
}

export function createMessagePackRuntimeDecoder(
  decode: MessagePackDecode,
  limits: RuntimeDecodeLimitsInput = {},
): (bytes: Uint8Array, path: string) => MessagePackValueReader {
  return (bytes) => decodeMessagePackValue(bytes, decode, limits);
}
