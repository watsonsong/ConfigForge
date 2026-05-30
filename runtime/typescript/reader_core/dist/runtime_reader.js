"use strict";
// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0
Object.defineProperty(exports, "__esModule", { value: true });
exports.RuntimeValueReader = exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = void 0;
exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0";
class RuntimeValueReader {
    constructor(value) {
        this.stack = [value];
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
        this.stack.push(fieldValue);
        return true;
    }
    enterIndex(index) {
        const value = this.currentValue();
        if (!Array.isArray(value) || index < 0 || index >= value.length) {
            throw new Error(`missing array index ${index}`);
        }
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
            return value;
        }
        throw new Error(`expected string value, got ${String(value)}`);
    }
    currentValue() {
        return this.stack[this.stack.length - 1];
    }
}
exports.RuntimeValueReader = RuntimeValueReader;
