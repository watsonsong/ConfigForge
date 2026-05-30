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

export const JsonValueReader = RuntimeValueReader;
export type JsonValueReader = RuntimeValueReader;

const utf8Decoder = new TextDecoder();

export function decodeJsonValue(
  bytes: Uint8Array | string,
  pathOrLimits: string | RuntimeDecodeLimitsInput = {},
): JsonValueReader {
  const limits = typeof pathOrLimits === "string" ? {} : pathOrLimits;
  const normalizedLimits = normalizeRuntimeDecodeLimits(limits);
  const byteLength = typeof bytes === "string" ? utf8ByteLength(bytes) : bytes.byteLength;
  checkRuntimeDecodeLimit("runtime file bytes", byteLength, normalizedLimits.maxFileBytes);
  const text = typeof bytes === "string" ? bytes : utf8Decoder.decode(bytes);
  return new JsonValueReader(JSON.parse(text), normalizedLimits);
}

function utf8ByteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}
