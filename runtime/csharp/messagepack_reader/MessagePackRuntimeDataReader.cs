// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;

namespace ConfigForge.Runtime.MessagePackReader;

public enum RuntimeDataErrorKind
{
    Io,
    Decode,
    SchemaMismatch,
    TypeMismatch,
    NotFound,
    Unsupported,
    InvalidPath,
}

public sealed class ConfigForgeRuntimeException : Exception
{
    public ConfigForgeRuntimeException(RuntimeDataErrorKind kind, string message) : base(message)
    {
        Kind = kind;
    }

    public ConfigForgeRuntimeException(RuntimeDataErrorKind kind, string message, Exception innerException) : base(message, innerException)
    {
        Kind = kind;
    }

    public RuntimeDataErrorKind Kind { get; }
    public string Code => $"CF_RT_{Kind.ToString().ToUpperInvariant()}";
    public string? Path { get; init; }
}


public class MessagePackRuntimeDataFileLoader
{
    private readonly string _root;

    public MessagePackRuntimeDataFileLoader(string root)
    {
        _root = root ?? throw new ArgumentNullException(nameof(root));
    }

    public int OpenCount { get; private set; }

    public MessagePackRuntimeDataReader OpenReader(string path)
    {
        OpenCount++;
        return new MessagePackRuntimeDataReader(File.ReadAllBytes(Path.Combine(_root, path + ".msgpack")));
    }
}

public class MessagePackRuntimeDataReader
{
    public const string TemplateVersion = "0.1.0";
    public const string SupportedConfigForgeReaderContract = "^1.0";
    public const string DistributionMode = "copyable-template";

    private readonly Stack<object?> _stack = new();

    public MessagePackRuntimeDataReader(byte[] bytes)
        : this(MessagePackValueParser.Parse(bytes))
    {
    }

    public MessagePackRuntimeDataReader(object? root)
    {
        _stack.Push(root);
    }

    public void EnterRequiredField(string name)
    {
        var current = _stack.Peek();
        if (current is Dictionary<string, object?> map && map.TryGetValue(name, out var value) && value is not null)
        {
            _stack.Push(value);
            return;
        }
        throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.NotFound, $"missing required field {name}");
    }

    public bool EnterOptionalField(string name)
    {
        var current = _stack.Peek();
        if (current is Dictionary<string, object?> map && map.TryGetValue(name, out var value) && value is not null)
        {
            _stack.Push(value);
            return true;
        }
        return false;
    }

    public void ExitField() => _stack.Pop();

    public ulong? SchemaRevision() => null;

    public int ReadRowCount()
    {
        var current = _stack.Peek();
        if (current is List<object?> currentList)
        {
            return currentList.Count;
        }
        return current is Dictionary<string, object?> map && map.TryGetValue("rows", out var rows) && rows is List<object?> list
            ? list.Count
            : 1;
    }

    public void EnterRow(int index)
    {
        var current = _stack.Peek();
        if (current is not Dictionary<string, object?> map || !map.TryGetValue("rows", out var rows) || rows is not List<object?> list)
        {
            throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.TypeMismatch, "expected MessagePack object with rows array");
        }
        _stack.Push(list[index]);
    }

    public void ExitRow() => _stack.Pop();

    public int ReadListLength()
    {
        var current = _stack.Peek();
        if (current is not List<object?> list)
        {
            throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.TypeMismatch, $"expected MessagePack array, got {TypeName(current)}");
        }
        return list.Count;
    }

    public void EnterListElement(int index)
    {
        if (_stack.Peek() is not List<object?> list)
        {
            throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.TypeMismatch, "expected MessagePack array");
        }
        _stack.Push(list[index]);
    }

    public void ExitListElement() => _stack.Pop();

    public void BeginRecord(int optionalSlotCount)
    {
    }

    public void EndRecord()
    {
    }

    public int ReadEnumIndex(IReadOnlyList<string> variants)
    {
        var value = ReadString();
        for (var index = 0; index < variants.Count; index++)
        {
            if (variants[index] == value)
            {
                return index;
            }
        }
        throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.Decode, $"unknown enum value: {value}");
    }

    public void EnsureExhausted()
    {
    }

    public long ReadInt()
    {
        var current = _stack.Peek();
        return current switch
        {
            long value => value,
            ulong value when value <= long.MaxValue => (long)value,
            int value => value,
            uint value => value,
            string value => long.Parse(value, CultureInfo.InvariantCulture),
            _ => throw new InvalidOperationException($"expected int-like value, got {TypeName(current)}"),
        };
    }

    public double ReadFloat()
    {
        var current = _stack.Peek();
        var parsed = current switch
        {
            double number => number,
            float number => number,
            long number => number,
            ulong number => number,
            int number => number,
            uint number => number,
            string text => double.Parse(text, CultureInfo.InvariantCulture),
            _ => throw new InvalidOperationException($"expected finite float-like value, got {TypeName(current)}"),
        };
        if (!double.IsFinite(parsed))
        {
            throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.TypeMismatch, "expected finite float-like value");
        }
        return parsed;
    }

    public bool ReadBool()
    {
        var current = _stack.Peek();
        return current switch
        {
            bool value => value,
            string value => ParseBoolString(value),
            _ => throw new InvalidOperationException($"expected bool-like value, got {TypeName(current)}"),
        };
    }

    public string ReadString()
    {
        var current = _stack.Peek();
        return current switch
        {
            string value => value,
            _ => throw new InvalidOperationException($"expected string value, got {TypeName(current)}"),
        };
    }

    private static bool ParseBoolString(string value)
    {
        return value.Trim() switch
        {
            "1" => true,
            "0" => false,
            "true" => true,
            "false" => false,
            _ => throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.Decode, $"invalid boolean string: {value}"),
        };
    }

    private static string TypeName(object? value) => value is null ? "nil" : value.GetType().Name;
}

internal sealed class MessagePackValueParser
{
    private readonly byte[] _bytes;
    private int _offset;

    private MessagePackValueParser(byte[] bytes)
    {
        _bytes = bytes ?? throw new ArgumentNullException(nameof(bytes));
    }

    public static object? Parse(byte[] bytes)
    {
        var parser = new MessagePackValueParser(bytes);
        var value = parser.ReadValue();
        if (parser._offset != bytes.Length)
        {
            throw new FormatException("trailing bytes in MessagePack payload");
        }
        return value;
    }

    private object? ReadValue()
    {
        var marker = ReadByte();
        if (marker <= 0x7f)
        {
            return (long)marker;
        }
        if ((marker & 0xf0) == 0x80)
        {
            return ReadMap(marker & 0x0f);
        }
        if ((marker & 0xf0) == 0x90)
        {
            return ReadArray(marker & 0x0f);
        }
        if ((marker & 0xe0) == 0xa0)
        {
            return ReadString(marker & 0x1f);
        }
        if (marker >= 0xe0)
        {
            return (long)(sbyte)marker;
        }

        return marker switch
        {
            0xc0 => null,
            0xc2 => false,
            0xc3 => true,
            0xca => (double)ReadFloat32(),
            0xcb => ReadFloat64(),
            0xcc => (long)ReadByte(),
            0xcd => (long)ReadUInt16(),
            0xce => (long)ReadUInt32(),
            0xcf => ReadUInt64Checked(),
            0xd0 => (long)(sbyte)ReadByte(),
            0xd1 => (long)ReadInt16(),
            0xd2 => (long)ReadInt32(),
            0xd3 => ReadInt64(),
            0xd9 => ReadString(ReadByte()),
            0xda => ReadString(ReadUInt16()),
            0xdb => ReadString(ReadLength32("string length")),
            0xdc => ReadArray(ReadUInt16()),
            0xdd => ReadArray(ReadLength32("array length")),
            0xde => ReadMap(ReadUInt16()),
            0xdf => ReadMap(ReadLength32("map length")),
            >= 0xc4 and <= 0xc9 => throw new FormatException("unsupported binary/ext value"),
            _ => throw new FormatException($"unsupported MessagePack marker 0x{marker:x2}"),
        };
    }

    private List<object?> ReadArray(int length)
    {
        var values = new List<object?>(length);
        for (var index = 0; index < length; index++)
        {
            values.Add(ReadValue());
        }
        return values;
    }

    private Dictionary<string, object?> ReadMap(int length)
    {
        var values = new Dictionary<string, object?>(length);
        for (var index = 0; index < length; index++)
        {
            var key = ReadValue();
            if (key is not string text)
            {
                throw new FormatException("MessagePack map key must be a string");
            }
            values[text] = ReadValue();
        }
        return values;
    }

    private string ReadString(int length)
    {
        var bytes = ReadExact(length);
        return Encoding.UTF8.GetString(bytes, 0, bytes.Length);
    }

    private byte ReadByte()
    {
        EnsureAvailable(1);
        return _bytes[_offset++];
    }

    private byte[] ReadExact(int length)
    {
        EnsureAvailable(length);
        var value = new byte[length];
        Buffer.BlockCopy(_bytes, _offset, value, 0, length);
        _offset += length;
        return value;
    }

    private void EnsureAvailable(int length)
    {
        if (length < 0 || _offset + length > _bytes.Length)
        {
            throw new FormatException("truncated MessagePack payload");
        }
    }

    private ushort ReadUInt16()
    {
        var bytes = ReadExact(2);
        return (ushort)((bytes[0] << 8) | bytes[1]);
    }

    private short ReadInt16() => unchecked((short)ReadUInt16());

    private uint ReadUInt32()
    {
        var bytes = ReadExact(4);
        return ((uint)bytes[0] << 24) | ((uint)bytes[1] << 16) | ((uint)bytes[2] << 8) | bytes[3];
    }

    private int ReadInt32() => unchecked((int)ReadUInt32());

    private ulong ReadUInt64()
    {
        var bytes = ReadExact(8);
        ulong value = 0;
        for (var index = 0; index < 8; index++)
        {
            value = (value << 8) | bytes[index];
        }
        return value;
    }

    private long ReadInt64() => unchecked((long)ReadUInt64());

    private ulong ReadUInt64Checked()
    {
        var value = ReadUInt64();
        if (value > long.MaxValue)
        {
            throw new OverflowException("MessagePack uint64 exceeds supported integer range");
        }
        return value;
    }

    private int ReadLength32(string label)
    {
        var value = ReadUInt32();
        if (value > int.MaxValue)
        {
            throw new OverflowException($"MessagePack {label} exceeds supported length");
        }
        return (int)value;
    }

    private float ReadFloat32()
    {
        var raw = ReadUInt32();
        return BitConverter.Int32BitsToSingle(unchecked((int)raw));
    }

    private double ReadFloat64()
    {
        var raw = ReadUInt64();
        return BitConverter.Int64BitsToDouble(unchecked((long)raw));
    }
}
