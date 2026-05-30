"use strict";
// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0
Object.defineProperty(exports, "__esModule", { value: true });
exports.RuntimeValueReader = exports.DEFAULT_RUNTIME_DECODE_LIMITS = exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = void 0;
exports.normalizeRuntimeDecodeLimits = normalizeRuntimeDecodeLimits;
exports.checkRuntimeDecodeLimit = checkRuntimeDecodeLimit;
exports.readRuntimeRowCount = readRuntimeRowCount;
exports.readRuntimeListLength = readRuntimeListLength;
exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0";
exports.DEFAULT_RUNTIME_DECODE_LIMITS = Object.freeze({
    maxFileBytes: 64 * 1024 * 1024,
    maxRows: 1000000,
    maxArrayLength: 1000000,
    maxStringBytes: 16 * 1024 * 1024,
    maxNestingDepth: 128,
});
function normalizeRuntimeDecodeLimits(limits = {}) {
    return {
        maxFileBytes: normalizeLimit("maxFileBytes", limits.maxFileBytes, exports.DEFAULT_RUNTIME_DECODE_LIMITS.maxFileBytes),
        maxRows: normalizeLimit("maxRows", limits.maxRows, exports.DEFAULT_RUNTIME_DECODE_LIMITS.maxRows),
        maxArrayLength: normalizeLimit("maxArrayLength", limits.maxArrayLength, exports.DEFAULT_RUNTIME_DECODE_LIMITS.maxArrayLength),
        maxStringBytes: normalizeLimit("maxStringBytes", limits.maxStringBytes, exports.DEFAULT_RUNTIME_DECODE_LIMITS.maxStringBytes),
        maxNestingDepth: normalizeLimit("maxNestingDepth", limits.maxNestingDepth, exports.DEFAULT_RUNTIME_DECODE_LIMITS.maxNestingDepth),
    };
}
function checkRuntimeDecodeLimit(label, actual, limit) {
    if (!Number.isSafeInteger(actual) || actual < 0) {
        throw new Error(`${label} must be a non-negative safe integer`);
    }
    if (actual > limit) {
        throw new Error(`${label} ${actual} exceeds decode limit ${limit}`);
    }
}
class RuntimeValueReader {
    constructor(value, limits = {}) {
        this.stack = [value];
        this.limits = normalizeRuntimeDecodeLimits(limits);
    }
    enterField(name) {
        const value = this.currentValue();
        if (value === null || typeof value !== "object" || Array.isArray(value)) {
            return false;
        }
        const object = value;
        if (!Object.prototype.hasOwnProperty.call(object, name)) {
            return false;
        }
        const fieldValue = object[name];
        if (fieldValue === null || fieldValue === undefined) {
            return false;
        }
        this.checkNestingDepth(this.stack.length);
        this.stack.push(fieldValue);
        return true;
    }
    enterIndex(index) {
        const value = this.currentValue();
        if (!Array.isArray(value) || index < 0 || index >= value.length) {
            throw new Error(`missing array index ${index}`);
        }
        this.checkNestingDepth(this.stack.length);
        this.stack.push(value[index]);
    }
    exit() {
        if (this.stack.length <= 1) {
            throw new Error("runtime reader cursor stack underflow");
        }
        this.stack.pop();
    }
    length() {
        const value = this.currentValue();
        if (Array.isArray(value)) {
            return value.length;
        }
        if (value !== null && typeof value === "object") {
            return Object.keys(value).length;
        }
        throw new Error("reader value is not array/object");
    }
    readRowCount() {
        const length = this.length();
        checkRuntimeDecodeLimit("row count", length, this.limits.maxRows);
        return length;
    }
    readListLength() {
        const length = this.length();
        checkRuntimeDecodeLimit("list length", length, this.limits.maxArrayLength);
        return length;
    }
    readArrayLength() {
        return this.readListLength();
    }
    decodeLimits() {
        return this.limits;
    }
    asNumber() {
        let parsed;
        const value = this.currentValue();
        if (typeof value === "number") {
            parsed = value;
        }
        else if (typeof value === "string") {
            if (value.trim().length === 0) {
                throw new Error("expected non-empty number-like string");
            }
            parsed = Number(value);
        }
        else {
            throw new Error(`expected number-like value, got ${String(value)}`);
        }
        if (!Number.isFinite(parsed)) {
            throw new Error(`expected finite number-like value, got ${String(value)}`);
        }
        return parsed;
    }
    asBool() {
        const value = this.currentValue();
        if (typeof value === "boolean") {
            return value;
        }
        if (typeof value === "string") {
            if (value === "true" || value === "1") {
                return true;
            }
            if (value === "false" || value === "0") {
                return false;
            }
        }
        throw new Error(`expected bool-like value, got ${String(value)}`);
    }
    asString() {
        const value = this.currentValue();
        if (typeof value === "string") {
            checkRuntimeDecodeLimit("string byte length", utf8ByteLength(value), this.limits.maxStringBytes);
            return value;
        }
        throw new Error(`expected string value, got ${String(value)}`);
    }
    currentValue() {
        return this.stack[this.stack.length - 1];
    }
    checkNestingDepth(depth) {
        checkRuntimeDecodeLimit("nesting depth", depth, this.limits.maxNestingDepth);
    }
}
exports.RuntimeValueReader = RuntimeValueReader;
function readRuntimeRowCount(reader) {
    if (!reader.readRowCount) {
        throw new Error("runtime reader does not support row count");
    }
    return reader.readRowCount();
}
function readRuntimeListLength(reader) {
    if (reader.readListLength) {
        return reader.readListLength();
    }
    if (reader.readArrayLength) {
        return reader.readArrayLength();
    }
    throw new Error("runtime reader does not support list length");
}
function normalizeLimit(label, value, fallback) {
    const resolved = value ?? fallback;
    if (!Number.isSafeInteger(resolved) || resolved < 0) {
        throw new Error(`${label} must be a non-negative safe integer`);
    }
    return resolved;
}
function utf8ByteLength(value) {
    if (typeof TextEncoder !== "undefined") {
        return new TextEncoder().encode(value).length;
    }
    let bytes = 0;
    for (let index = 0; index < value.length; index += 1) {
        const code = value.charCodeAt(index);
        if (code <= 0x7f) {
            bytes += 1;
        }
        else if (code <= 0x7ff) {
            bytes += 2;
        }
        else if (code >= 0xd800 && code <= 0xdbff && index + 1 < value.length) {
            const next = value.charCodeAt(index + 1);
            if (next >= 0xdc00 && next <= 0xdfff) {
                bytes += 4;
                index += 1;
            }
            else {
                bytes += 3;
            }
        }
        else {
            bytes += 3;
        }
    }
    return bytes;
}
