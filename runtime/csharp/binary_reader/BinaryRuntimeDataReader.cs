// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace ConfigForge.Runtime.BinaryReader;

public class BinaryRuntimeDataFileLoader
{
    private readonly string _root;

    public BinaryRuntimeDataFileLoader(string root)
    {
        _root = root ?? throw new ArgumentNullException(nameof(root));
    }

    public int OpenCount { get; private set; }

    public BinaryRuntimeDataReader OpenReader(string path)
    {
        OpenCount++;
        return new BinaryRuntimeDataReader(File.ReadAllBytes(Path.Combine(_root, path + ".bin")));
    }
}

public class BinaryRuntimeDataReader
{
    public const string TemplateVersion = "0.1.0";
    public const string SupportedConfigForgeReaderContract = "^1.0";
    public const string DistributionMode = "copyable-template";

    private static readonly UTF8Encoding StrictUtf8 = new(false, true);

    private readonly byte[] _bytes;
    private readonly List<RecordState> _records = new();
    private int _offset = 16;
    private int _nextRowIndex;
    private ulong _schemaRevision;

    public BinaryRuntimeDataReader(byte[] bytes)
    {
        _bytes = bytes ?? throw new ArgumentNullException(nameof(bytes));
        ParseHeader();
    }

    public void EnterRequiredField(string name)
    {
    }

    public bool EnterOptionalField(string name) => ReadOptionalPresent();

    public void ExitField()
    {
    }

    public ulong? SchemaRevision() => _schemaRevision;

    public int ReadRowCount()
    {
        var rowCount = CheckedInt(ReadVaruint(), "row count");
        _nextRowIndex = 0;
        return rowCount;
    }

    public void EnterRow(int index)
    {
        if (index != _nextRowIndex)
        {
            throw new InvalidOperationException($"binary reader rows must be read sequentially: expected {_nextRowIndex}, got {index}");
        }
        _nextRowIndex++;
    }

    public void ExitRow()
    {
    }

    public int ReadListLength() => CheckedInt(ReadVaruint(), "list length");

    public void EnterListElement(int index)
    {
    }

    public void ExitListElement()
    {
    }

    public void BeginRecord(int optionalSlotCount)
    {
        if (optionalSlotCount < 0)
        {
            throw new InvalidOperationException("optional slot count must be non-negative");
        }
        var byteCount = (optionalSlotCount + 7) / 8;
        var bitmap = ReadExact(byteCount);
        ValidateOptionalBitmap(bitmap, optionalSlotCount);
        _records.Add(new RecordState(bitmap, 0));
    }

    public void EndRecord()
    {
        if (_records.Count == 0)
        {
            throw new InvalidOperationException("binary reader record stack underflow");
        }
        _records.RemoveAt(_records.Count - 1);
    }

    public int ReadEnumIndex(IReadOnlyList<string> variants)
    {
        var ordinal = CheckedInt(ReadVaruint(), "enum ordinal");
        if (ordinal < 0 || ordinal >= variants.Count)
        {
            throw new InvalidOperationException("enum ordinal is out of range");
        }
        return ordinal;
    }

    public void EnsureExhausted()
    {
        if (_offset != _bytes.Length)
        {
            throw new InvalidOperationException($"trailing bytes in binary runtime data: {_bytes.Length - _offset} bytes remaining");
        }
    }

    public long ReadInt()
    {
        var encoded = ReadVaruint();
        var value = unchecked((long)(encoded >> 1));
        return (encoded & 1UL) == 0 ? value : ~value;
    }

    public double ReadFloat()
    {
        var value = BitConverter.Int64BitsToDouble(unchecked((long)ReadLe64(ReadExact(8))));
        if (!double.IsFinite(value))
        {
            throw new InvalidOperationException("non-finite float in binary runtime data");
        }
        return value;
    }

    public bool ReadBool()
    {
        var value = ReadExact(1)[0];
        return value switch
        {
            0 => false,
            1 => true,
            _ => throw new InvalidOperationException($"invalid bool byte {value}"),
        };
    }

    public string ReadString()
    {
        var length = CheckedInt(ReadVaruint(), "string length");
        return StrictUtf8.GetString(ReadExact(length));
    }

    private bool ReadOptionalPresent()
    {
        if (_records.Count == 0)
        {
            throw new InvalidOperationException("binary reader is not inside a record");
        }
        var record = _records[^1];
        var slotIndex = record.NextOptionalSlot++;
        _records[^1] = record;
        var byteIndex = slotIndex / 8;
        if (byteIndex >= record.Bitmap.Length)
        {
            throw new InvalidOperationException("optional slot is out of current record bitmap");
        }
        return (record.Bitmap[byteIndex] & (1 << (slotIndex % 8))) != 0;
    }

    private static void ValidateOptionalBitmap(byte[] bitmap, int optionalSlotCount)
    {
        var usedBitsInLastByte = optionalSlotCount % 8;
        if (bitmap.Length == 0 || usedBitsInLastByte == 0)
        {
            return;
        }
        var unusedMask = 0xff << usedBitsInLastByte;
        if ((bitmap[^1] & unusedMask) != 0)
        {
            throw new InvalidOperationException("invalid optional bitmap padding bits");
        }
    }

    private void ParseHeader()
    {
        if (_bytes.Length < 16)
        {
            throw new InvalidOperationException("truncated binary runtime data header");
        }
        if (_bytes[0] != 'C' || _bytes[1] != 'F' || _bytes[2] != 'G' || _bytes[3] != 'B')
        {
            throw new InvalidOperationException("invalid binary runtime data magic");
        }
        var version = ReadLe16(4);
        if (version != 1)
        {
            throw new InvalidOperationException($"unsupported binary runtime data version {version}");
        }
        var headerSize = ReadLe16(6);
        if (headerSize != 16)
        {
            throw new InvalidOperationException($"unsupported binary runtime data header size {headerSize}");
        }
        _schemaRevision = ReadLe64(8);
    }

    private ushort ReadLe16(int offset)
    {
        return (ushort)(_bytes[offset] | (_bytes[offset + 1] << 8));
    }

    private ulong ReadLe64(int offset)
    {
        ulong value = 0;
        for (var index = 0; index < 8; index++)
        {
            value |= (ulong)_bytes[offset + index] << (index * 8);
        }
        return value;
    }

    private static ulong ReadLe64(byte[] bytes)
    {
        ulong value = 0;
        for (var index = 0; index < 8; index++)
        {
            value |= (ulong)bytes[index] << (index * 8);
        }
        return value;
    }

    private ulong ReadVaruint()
    {
        ulong value = 0;
        var shift = 0;
        for (var index = 0; index < 10; index++)
        {
            var valueByte = ReadExact(1)[0];
            if (index == 9 && (valueByte & 0x7f) > 1)
            {
                throw new InvalidOperationException("varuint exceeds u64");
            }
            value |= (ulong)(valueByte & 0x7f) << shift;
            if ((valueByte & 0x80) == 0)
            {
                return value;
            }
            shift += 7;
        }
        throw new InvalidOperationException("malformed varuint");
    }

    private byte[] ReadExact(int length)
    {
        if (length < 0 || _offset + length < _offset || _offset + length > _bytes.Length)
        {
            throw new InvalidOperationException("truncated binary runtime data");
        }
        var result = new byte[length];
        Array.Copy(_bytes, _offset, result, 0, length);
        _offset += length;
        return result;
    }

    private static int CheckedInt(ulong value, string label)
    {
        if (value > int.MaxValue)
        {
            throw new InvalidOperationException($"{label} exceeds int range");
        }
        return (int)value;
    }

    private sealed class RecordState
    {
        public RecordState(byte[] bitmap, int nextOptionalSlot)
        {
            Bitmap = bitmap;
            NextOptionalSlot = nextOptionalSlot;
        }

        public byte[] Bitmap { get; }
        public int NextOptionalSlot { get; set; }
    }
}
