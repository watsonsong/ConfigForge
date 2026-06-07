// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

package configforge.runtime.messagepack;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public final class ConfigForgeMessagePackParser {
    private final byte[] bytes;
    private int offset;

    private ConfigForgeMessagePackParser(byte[] bytes) {
        this.bytes = bytes;
    }

    public static Object parse(byte[] bytes) {
        ConfigForgeMessagePackParser parser = new ConfigForgeMessagePackParser(bytes);
        Object value = parser.readValue();
        if (parser.offset != bytes.length) {
            throw new IllegalArgumentException("trailing bytes in MessagePack payload");
        }
        return value;
    }

    private Object readValue() {
        int marker = readByte();
        if (marker <= 0x7f) {
            return Long.valueOf(marker);
        }
        if ((marker & 0xf0) == 0x80) {
            return readMap(marker & 0x0f);
        }
        if ((marker & 0xf0) == 0x90) {
            return readArray(marker & 0x0f);
        }
        if ((marker & 0xe0) == 0xa0) {
            return readString(marker & 0x1f);
        }
        if (marker >= 0xe0) {
            return Long.valueOf((byte) marker);
        }

        switch (marker) {
            case 0xc0: return null;
            case 0xc2: return Boolean.FALSE;
            case 0xc3: return Boolean.TRUE;
            case 0xca: return Double.valueOf(Float.intBitsToFloat((int) readUInt32()));
            case 0xcb: return Double.valueOf(Double.longBitsToDouble(readInt64Bits()));
            case 0xcc: return Long.valueOf(readByte());
            case 0xcd: return Long.valueOf(readUInt16());
            case 0xce: return Long.valueOf(readUInt32());
            case 0xcf: return Long.valueOf(readUInt64Checked());
            case 0xd0: return Long.valueOf((byte) readByte());
            case 0xd1: return Long.valueOf((short) readUInt16());
            case 0xd2: return Long.valueOf((int) readUInt32());
            case 0xd3: return Long.valueOf(readInt64Bits());
            case 0xd9: return readString(readByte());
            case 0xda: return readString(readUInt16());
            case 0xdb: return readString(readLength32("string length"));
            case 0xdc: return readArray(readUInt16());
            case 0xdd: return readArray(readLength32("array length"));
            case 0xde: return readMap(readUInt16());
            case 0xdf: return readMap(readLength32("map length"));
            default:
                if (marker >= 0xc4 && marker <= 0xc9) {
                    throw new IllegalArgumentException("unsupported binary/ext value");
                }
                throw new IllegalArgumentException(String.format("unsupported MessagePack marker 0x%02x", marker));
        }
    }

    private List<Object> readArray(int length) {
        List<Object> result = new ArrayList<Object>(length);
        for (int index = 0; index < length; index++) {
            result.add(readValue());
        }
        return result;
    }

    private Map<String, Object> readMap(int length) {
        Map<String, Object> result = new HashMap<String, Object>(length);
        for (int index = 0; index < length; index++) {
            Object key = readValue();
            if (!(key instanceof String)) {
                throw new IllegalArgumentException("MessagePack map key must be a string");
            }
            result.put((String) key, readValue());
        }
        return result;
    }

    private String readString(int length) {
        byte[] value = readExact(length);
        return new String(value, StandardCharsets.UTF_8);
    }

    private int readByte() {
        ensureAvailable(1);
        return bytes[offset++] & 0xff;
    }

    private byte[] readExact(int length) {
        ensureAvailable(length);
        byte[] value = new byte[length];
        System.arraycopy(bytes, offset, value, 0, length);
        offset += length;
        return value;
    }

    private void ensureAvailable(int length) {
        if (length < 0 || offset + length > bytes.length) {
            throw new IllegalArgumentException("truncated MessagePack payload");
        }
    }

    private int readUInt16() {
        byte[] value = readExact(2);
        return ((value[0] & 0xff) << 8) | (value[1] & 0xff);
    }

    private long readUInt32() {
        byte[] value = readExact(4);
        return ((long) (value[0] & 0xff) << 24)
            | ((long) (value[1] & 0xff) << 16)
            | ((long) (value[2] & 0xff) << 8)
            | (long) (value[3] & 0xff);
    }

    private long readInt64Bits() {
        byte[] value = readExact(8);
        long result = 0L;
        for (int index = 0; index < 8; index++) {
            result = (result << 8) | (long) (value[index] & 0xff);
        }
        return result;
    }

    private long readUInt64Checked() {
        long value = readInt64Bits();
        if (value < 0) {
            throw new ArithmeticException("MessagePack uint64 exceeds supported integer range");
        }
        return value;
    }

    private int readLength32(String label) {
        long value = readUInt32();
        if (value > Integer.MAX_VALUE) {
            throw new ArithmeticException("MessagePack " + label + " exceeds supported length");
        }
        return (int) value;
    }
}
