// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

import {
  checkRuntimeDecodeLimit,
  RuntimeDataError,
  RuntimeDecodeLimitsInput,
  RuntimeValueReader,
  normalizeRuntimeDecodeLimits,
  SUPPORTED_CONFIG_FORGE_READER_CONTRACT,
} from "@config-forge/runtime-reader-core";
import { decode as decodeMessagePackBackend } from "@msgpack/msgpack";

export { SUPPORTED_CONFIG_FORGE_READER_CONTRACT };

export type MessagePackDecode = (bytes: Uint8Array) => unknown;

export const MessagePackValueReader = RuntimeValueReader;
export type MessagePackValueReader = RuntimeValueReader;

export function decodeMessagePackValue(
  bytes: Uint8Array,
  limits?: RuntimeDecodeLimitsInput,
): MessagePackValueReader;
export function decodeMessagePackValue(
  bytes: Uint8Array,
  decode: MessagePackDecode,
  limits?: RuntimeDecodeLimitsInput,
): MessagePackValueReader;
export function decodeMessagePackValue(
  bytes: Uint8Array,
  decodeOrLimits: MessagePackDecode | RuntimeDecodeLimitsInput = decodeMessagePackBackend,
  limits: RuntimeDecodeLimitsInput = {},
): MessagePackValueReader {
  const decode = typeof decodeOrLimits === "function" ? decodeOrLimits : decodeMessagePackBackend;
  const decodeLimits = typeof decodeOrLimits === "function" ? limits : decodeOrLimits;
  const normalizedLimits = normalizeRuntimeDecodeLimits(decodeLimits);
  checkRuntimeDecodeLimit("runtime file bytes", bytes.byteLength, normalizedLimits.maxFileBytes);
  const decoded = decodeMessagePackBytes(bytes, decode);
  rejectUnsupportedMessagePackValue(decoded);
  return new MessagePackValueReader(decoded, normalizedLimits);
}

export function createMessagePackRuntimeDecoder(
  limits?: RuntimeDecodeLimitsInput,
): (bytes: Uint8Array, path: string) => MessagePackValueReader;
export function createMessagePackRuntimeDecoder(
  decode: MessagePackDecode,
  limits?: RuntimeDecodeLimitsInput,
): (bytes: Uint8Array, path: string) => MessagePackValueReader;
export function createMessagePackRuntimeDecoder(
  decodeOrLimits: MessagePackDecode | RuntimeDecodeLimitsInput = decodeMessagePackBackend,
  limits: RuntimeDecodeLimitsInput = {},
): (bytes: Uint8Array, path: string) => MessagePackValueReader {
  return (bytes) => typeof decodeOrLimits === "function"
    ? decodeMessagePackValue(bytes, decodeOrLimits, limits)
    : decodeMessagePackValue(bytes, decodeOrLimits);
}

function decodeMessagePackBytes(bytes: Uint8Array, decode: MessagePackDecode): unknown {
  try {
    return decode(bytes);
  } catch (error) {
    throw normalizeMessagePackDecodeError(error);
  }
}

function normalizeMessagePackDecodeError(error: unknown): RuntimeDataError {
  if (error instanceof RuntimeDataError) {
    return error;
  }
  const message = error instanceof Error ? error.message : String(error);
  const lowerMessage = message.toLowerCase();
  if (lowerMessage.includes("unsupported non-string map key") || lowerMessage.includes("unsupported binary/ext")) {
    return RuntimeDataError.unsupported(message, { expected: "MessagePack runtime value", actual: message });
  }
  return RuntimeDataError.decode(message, { expected: "valid MessagePack payload", actual: message });
}

function rejectUnsupportedMessagePackValue(value: unknown): void {
  const seen = new Set<object>();
  const visit = (candidate: unknown): void => {
    if (candidate instanceof Uint8Array || candidate instanceof ArrayBuffer) {
      throw RuntimeDataError.unsupported("unsupported binary/ext value", {
        expected: "MessagePack runtime value without bin/ext",
        actual: "binary/ext value",
      });
    }
    if (ArrayBuffer.isView(candidate)) {
      throw RuntimeDataError.unsupported("unsupported binary/ext value", {
        expected: "MessagePack runtime value without bin/ext",
        actual: "binary/ext value",
      });
    }
    if (candidate === null || typeof candidate !== "object") {
      return;
    }
    if (seen.has(candidate)) {
      return;
    }
    seen.add(candidate);
    if (candidate instanceof Map) {
      for (const [key, entryValue] of candidate.entries()) {
        if (typeof key !== "string") {
          throw RuntimeDataError.unsupported("unsupported non-string map key", {
            expected: "MessagePack map with string keys",
            actual: typeof key,
          });
        }
        visit(entryValue);
      }
      return;
    }
    if (Array.isArray(candidate)) {
      for (const entry of candidate) {
        visit(entry);
      }
      return;
    }
    for (const entry of Object.values(candidate as Record<string, unknown>)) {
      visit(entry);
    }
  };
  visit(value);
}
