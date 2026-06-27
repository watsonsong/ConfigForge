"use strict";
// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0
Object.defineProperty(exports, "__esModule", { value: true });
exports.RuntimeValueReader = exports.DEFAULT_RUNTIME_DECODE_LIMITS = exports.RuntimeDataError = exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = void 0;
exports.normalizeRuntimeDecodeLimits = normalizeRuntimeDecodeLimits;
exports.checkRuntimeDecodeLimit = checkRuntimeDecodeLimit;
exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0";
class RuntimeDataError extends Error {
    constructor(kind, message, context = {}, options) {
        super(message);
        this.name = "RuntimeDataError";
        this.kind = kind;
        this.context = context;
        if (options && "cause" in options) {
            this.cause = options.cause;
        }
    }
    static io(message, cause, context = {}) {
        return new RuntimeDataError("Io", message, context, { cause });
    }
    static decode(message, context = {}) {
        return new RuntimeDataError("Decode", message, context);
    }
    static schemaMismatch(message, context = {}) {
        return new RuntimeDataError("SchemaMismatch", message, context);
    }
    static typeMismatch(message, context = {}) {
        return new RuntimeDataError("TypeMismatch", message, context);
    }
    static notFound(message, context = {}) {
        return new RuntimeDataError("NotFound", message, context);
    }
    static unsupported(message, context = {}) {
        return new RuntimeDataError("Unsupported", message, context);
    }
    static invalidPath(message, context = {}) {
        return new RuntimeDataError("InvalidPath", message, context);
    }
}
exports.RuntimeDataError = RuntimeDataError;
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
        throw RuntimeDataError.decode(`${label} must be a non-negative safe integer`, { expected: "non-negative safe integer", actual: String(actual) });
    }
    if (actual > limit) {
        throw RuntimeDataError.decode(`${label} ${actual} exceeds decode limit ${limit}`, { expected: `<= ${limit}`, actual: String(actual) });
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
            throw RuntimeDataError.notFound(`missing array index ${index}`, { expected: "array index", actual: runtimeTypeOf(value) });
        }
        this.checkNestingDepth(this.stack.length);
        this.stack.push(value[index]);
    }
    exit() {
        if (this.stack.length <= 1) {
            throw RuntimeDataError.decode("runtime reader cursor stack underflow");
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
        throw RuntimeDataError.typeMismatch("reader value is not array/object", { expected: "array/object", actual: runtimeTypeOf(value) });
    }
    enterRequiredField(name) {
        if (!this.enterField(name)) {
            throw RuntimeDataError.notFound(`missing required field ${name}`, { expected: `field ${name}`, actual: "missing/null" });
        }
    }
    enterOptionalField(name) {
        return this.enterField(name);
    }
    exitField() {
        this.exit();
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
    readEnumIndex(variants) {
        const value = this.asString();
        const index = variants.indexOf(value);
        if (index < 0) {
            throw RuntimeDataError.decode(`unknown enum value ${value}`, { expected: variants.join(" | "), actual: value });
        }
        return index;
    }
    arrayElementsArePositional() {
        return false;
    }
    beginRecord(_optionalSlotCount) {
    }
    endRecord() {
    }
    asInt64() {
        let parsed;
        const value = this.currentValue();
        if (typeof value === "number") {
            parsed = value;
        }
        else if (typeof value === "string") {
            if (value.trim().length === 0) {
                throw RuntimeDataError.typeMismatch("expected non-empty number-like string", { expected: "number-like string", actual: "empty string" });
            }
            parsed = Number(value);
        }
        else {
            throw RuntimeDataError.typeMismatch(`expected number-like value, got ${String(value)}`, { expected: "number-like value", actual: runtimeTypeOf(value) });
        }
        if (!Number.isFinite(parsed)) {
            throw RuntimeDataError.typeMismatch(`expected finite number-like value, got ${String(value)}`, { expected: "finite number-like value", actual: String(value) });
        }
        if (!Number.isSafeInteger(parsed)) {
            throw RuntimeDataError.decode(`expected safe integer-like value, got ${String(value)}`, { expected: "safe integer-like value", actual: String(value) });
        }
        return parsed;
    }
    asFloat64() {
        return this.asFiniteNumber();
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
        throw RuntimeDataError.decode(`expected bool-like value, got ${String(value)}`, { expected: "bool-like value", actual: String(value) });
    }
    asString() {
        const value = this.currentValue();
        if (typeof value === "string") {
            checkRuntimeDecodeLimit("string byte length", utf8ByteLength(value), this.limits.maxStringBytes);
            return value;
        }
        throw RuntimeDataError.typeMismatch(`expected string value, got ${String(value)}`, { expected: "string", actual: runtimeTypeOf(value) });
    }
    ensureExhausted() {
    }
    asFiniteNumber() {
        let parsed;
        const value = this.currentValue();
        if (typeof value === "number") {
            parsed = value;
        }
        else if (typeof value === "string") {
            if (value.trim().length === 0) {
                throw RuntimeDataError.typeMismatch("expected non-empty number-like string", { expected: "number-like string", actual: "empty string" });
            }
            parsed = Number(value);
        }
        else {
            throw RuntimeDataError.typeMismatch(`expected number-like value, got ${String(value)}`, { expected: "number-like value", actual: runtimeTypeOf(value) });
        }
        if (!Number.isFinite(parsed)) {
            throw RuntimeDataError.typeMismatch(`expected finite number-like value, got ${String(value)}`, { expected: "finite number-like value", actual: String(value) });
        }
        return parsed;
    }
    currentValue() {
        return this.stack[this.stack.length - 1];
    }
    checkNestingDepth(depth) {
        checkRuntimeDecodeLimit("nesting depth", depth, this.limits.maxNestingDepth);
    }
}
exports.RuntimeValueReader = RuntimeValueReader;
function normalizeLimit(label, value, fallback) {
    const resolved = value ?? fallback;
    if (!Number.isSafeInteger(resolved) || resolved < 0) {
        throw RuntimeDataError.decode(`${label} must be a non-negative safe integer`, { expected: "non-negative safe integer", actual: String(resolved) });
    }
    return resolved;
}
function runtimeTypeOf(value) {
    if (value === null) {
        return "null";
    }
    if (Array.isArray(value)) {
        return "array";
    }
    return typeof value;
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
