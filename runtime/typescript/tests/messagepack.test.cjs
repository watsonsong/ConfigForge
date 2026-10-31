// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

const assert = require('node:assert/strict');
const test = require('node:test');
const { encode } = require('@msgpack/msgpack');
const { decodeMessagePackValue } = require('@config-forge/runtime-reader-messagepack');

test('default backend reads encoded rows without an injected decoder', () => {
  const reader = decodeMessagePackValue(encode([{ id: 42, name: 'Potion' }]));
  assert.equal(reader.readRowCount(), 1);
  reader.enterIndex(0);
  reader.enterRequiredField('id');
  assert.equal(reader.asInt64(), 42);
  reader.exitField();
  reader.enterRequiredField('name');
  assert.equal(reader.asString(), 'Potion');
});

test('default backend normalizes malformed data and rejects binary values', () => {
  assert.throws(() => decodeMessagePackValue(Uint8Array.of(0xc1)), { code: 'CF_RT_DECODE' });
  assert.throws(() => decodeMessagePackValue(encode(new Uint8Array([1]))), { code: 'CF_RT_UNSUPPORTED' });
});

test('file size limit is checked before invoking the backend', () => {
  assert.throws(() => decodeMessagePackValue(encode([1, 2]), { maxFileBytes: 1 }), { code: 'CF_RT_DECODE' });
});
