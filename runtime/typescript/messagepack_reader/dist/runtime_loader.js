"use strict";
// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0
Object.defineProperty(exports, "__esModule", { value: true });
exports.MessagePackValueReader = exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = void 0;
exports.decodeMessagePackValue = decodeMessagePackValue;
exports.createMessagePackRuntimeDecoder = createMessagePackRuntimeDecoder;
const runtime_reader_core_1 = require("@config-forge/runtime-reader-core");
Object.defineProperty(exports, "SUPPORTED_CONFIG_FORGE_READER_CONTRACT", { enumerable: true, get: function () { return runtime_reader_core_1.SUPPORTED_CONFIG_FORGE_READER_CONTRACT; } });
const msgpack_1 = require("@msgpack/msgpack");
exports.MessagePackValueReader = runtime_reader_core_1.RuntimeValueReader;
function decodeMessagePackValue(bytes, decodeOrLimits = msgpack_1.decode, limits = {}) {
    const decode = typeof decodeOrLimits === "function" ? decodeOrLimits : msgpack_1.decode;
    const decodeLimits = typeof decodeOrLimits === "function" ? limits : decodeOrLimits;
    const normalizedLimits = (0, runtime_reader_core_1.normalizeRuntimeDecodeLimits)(decodeLimits);
    (0, runtime_reader_core_1.checkRuntimeDecodeLimit)("runtime file bytes", bytes.byteLength, normalizedLimits.maxFileBytes);
    const decoded = decodeMessagePackBytes(bytes, decode);
    rejectUnsupportedMessagePackValue(decoded);
    return new exports.MessagePackValueReader(decoded, normalizedLimits);
}
function createMessagePackRuntimeDecoder(decodeOrLimits = msgpack_1.decode, limits = {}) {
    return (bytes) => typeof decodeOrLimits === "function"
        ? decodeMessagePackValue(bytes, decodeOrLimits, limits)
        : decodeMessagePackValue(bytes, decodeOrLimits);
}
function decodeMessagePackBytes(bytes, decode) {
    try {
        return decode(bytes);
    }
    catch (error) {
        throw normalizeMessagePackDecodeError(error);
    }
}
function normalizeMessagePackDecodeError(error) {
    if (error instanceof runtime_reader_core_1.RuntimeDataError) {
        return error;
    }
    const message = error instanceof Error ? error.message : String(error);
    const lowerMessage = message.toLowerCase();
    if (lowerMessage.includes("unsupported non-string map key") || lowerMessage.includes("unsupported binary/ext")) {
        return runtime_reader_core_1.RuntimeDataError.unsupported(message, { expected: "MessagePack runtime value", actual: message });
    }
    return runtime_reader_core_1.RuntimeDataError.decode(message, { expected: "valid MessagePack payload", actual: message });
}
function rejectUnsupportedMessagePackValue(value) {
    const seen = new Set();
    const visit = (candidate) => {
        if (candidate instanceof Uint8Array || candidate instanceof ArrayBuffer) {
            throw runtime_reader_core_1.RuntimeDataError.unsupported("unsupported binary/ext value", {
                expected: "MessagePack runtime value without bin/ext",
                actual: "binary/ext value",
            });
        }
        if (ArrayBuffer.isView(candidate)) {
            throw runtime_reader_core_1.RuntimeDataError.unsupported("unsupported binary/ext value", {
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
                    throw runtime_reader_core_1.RuntimeDataError.unsupported("unsupported non-string map key", {
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
        for (const entry of Object.values(candidate)) {
            visit(entry);
        }
    };
    visit(value);
}
