// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

package configforge.runtime.binary;

import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

public class ConfigForgeBinaryRuntimeDataReader {
    public static final String TEMPLATE_VERSION = "0.1.0";
    public static final String SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0";
    public static final String DISTRIBUTION_MODE = "copyable-template";
    public static final String PARSER_BACKEND = "configforge-binary";
    public static final String PARSER_BACKEND_VERSION = "0.1";
    public static final String ADAPTER_SUPPORT_LEVEL = "stable";

    private final byte[] bytes;
    private final List<RecordState> records = new ArrayList<RecordState>();
    private int offset = 16;
    private int nextRowIndex;
    private long schemaRevision;

    public ConfigForgeBinaryRuntimeDataReader(byte[] bytes) {
        if (bytes == null) {
            throw new IllegalArgumentException("bytes are required");
        }
        this.bytes = bytes;
        parseHeader();
    }

    public void enterRequiredField(String name) {
    }

    public boolean enterOptionalField(String name) {
        return readOptionalPresent();
    }

    public void exitField() {
    }

    public Long schemaRevision() {
        return Long.valueOf(schemaRevision);
    }

    public int readRowCount() {
        int rowCount = checkedInt(readVaruint(), "row count");
        nextRowIndex = 0;
        return rowCount;
    }

    public void enterRow(int index) {
        if (index != nextRowIndex) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.InvalidPath, "binary reader rows must be read sequentially: expected " + nextRowIndex + ", got " + index);
        }
        nextRowIndex++;
    }

    public void exitRow() {
    }

    public int readListLength() {
        return checkedInt(readVaruint(), "list length");
    }

    public void enterListElement(int index) {
    }

    public void exitListElement() {
    }

    public void beginRecord(int optionalSlotCount) {
        if (optionalSlotCount < 0) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.InvalidPath, "optional slot count must be non-negative");
        }
        int byteCount = (optionalSlotCount + 7) / 8;
        byte[] bitmap = readExact(byteCount);
        validateOptionalBitmap(bitmap, optionalSlotCount);
        records.add(new RecordState(bitmap, 0));
    }

    public void endRecord() {
        if (records.isEmpty()) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "binary reader record stack underflow");
        }
        records.remove(records.size() - 1);
    }

    public int readEnumIndex(String[] variants) {
        int ordinal = checkedInt(readVaruint(), "enum ordinal");
        if (ordinal < 0 || ordinal >= variants.length) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "enum ordinal is out of range");
        }
        return ordinal;
    }

    public void ensureExhausted() {
        if (offset != bytes.length) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "trailing bytes in binary runtime data: " + (bytes.length - offset) + " bytes remaining");
        }
    }

    public long readInt() {
        long encoded = readVaruint();
        return (encoded >>> 1) ^ -(encoded & 1L);
    }

    public double readFloat() {
        double value = Double.longBitsToDouble(readLe64(readExact(8), 0));
        if (!Double.isFinite(value)) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.TypeMismatch, "non-finite float in binary runtime data");
        }
        return value;
    }

    public boolean readBool() {
        int value = readExact(1)[0] & 0xff;
        if (value == 0) {
            return false;
        }
        if (value == 1) {
            return true;
        }
        throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "invalid bool byte " + value);
    }

    public String readString() {
        int length = checkedInt(readVaruint(), "string length");
        try {
            return StandardCharsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(readExact(length))).toString();
        } catch (CharacterCodingException error) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "invalid UTF-8 string in binary runtime data", error);
        }
    }

    private boolean readOptionalPresent() {
        if (records.isEmpty()) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.InvalidPath, "binary reader is not inside a record");
        }
        RecordState record = records.get(records.size() - 1);
        int slotIndex = record.nextOptionalSlot++;
        int byteIndex = slotIndex / 8;
        if (byteIndex >= record.bitmap.length) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "optional slot is out of current record bitmap");
        }
        return (record.bitmap[byteIndex] & (1 << (slotIndex % 8))) != 0;
    }

    private void validateOptionalBitmap(byte[] bitmap, int optionalSlotCount) {
        int usedBitsInLastByte = optionalSlotCount % 8;
        if (bitmap.length == 0 || usedBitsInLastByte == 0) {
            return;
        }
        int unusedMask = 0xff << usedBitsInLastByte;
        if ((bitmap[bitmap.length - 1] & unusedMask) != 0) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "invalid optional bitmap padding bits");
        }
    }

    private void parseHeader() {
        if (bytes.length < 16) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "truncated binary runtime data header");
        }
        if (bytes[0] != 'C' || bytes[1] != 'F' || bytes[2] != 'G' || bytes[3] != 'B') {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "invalid binary runtime data magic");
        }
        int version = readLe16(4);
        if (version != 1) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.SchemaMismatch, "unsupported binary runtime data version " + version);
        }
        int headerSize = readLe16(6);
        if (headerSize != 16) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.SchemaMismatch, "unsupported binary runtime data header size " + headerSize);
        }
        schemaRevision = readLe64(bytes, 8);
    }

    private int readLe16(int position) {
        return (bytes[position] & 0xff) | ((bytes[position + 1] & 0xff) << 8);
    }

    private long readLe64(byte[] source, int position) {
        long value = 0L;
        for (int index = 0; index < 8; index++) {
            value |= ((long) source[position + index] & 0xffL) << (index * 8);
        }
        return value;
    }

    private long readVaruint() {
        long value = 0L;
        int shift = 0;
        for (int index = 0; index < 10; index++) {
            int valueByte = readExact(1)[0] & 0xff;
            if (index == 9 && (valueByte & 0x7f) > 1) {
                throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "varuint exceeds u64");
            }
            value |= ((long) (valueByte & 0x7f)) << shift;
            if ((valueByte & 0x80) == 0) {
                return value;
            }
            shift += 7;
        }
        throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "malformed varuint");
    }

    private byte[] readExact(int length) {
        int end = offset + length;
        if (length < 0 || end < offset || end > bytes.length) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "truncated binary runtime data");
        }
        byte[] value = Arrays.copyOfRange(bytes, offset, end);
        offset = end;
        return value;
    }

    private int checkedInt(long value, String label) {
        if (Long.compareUnsigned(value, Integer.MAX_VALUE) > 0) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, label + " exceeds int range");
        }
        return (int) value;
    }

    private static final class RecordState {
        final byte[] bitmap;
        int nextOptionalSlot;

        RecordState(byte[] bitmap, int nextOptionalSlot) {
            this.bitmap = bitmap;
            this.nextOptionalSlot = nextOptionalSlot;
        }
    }
}
