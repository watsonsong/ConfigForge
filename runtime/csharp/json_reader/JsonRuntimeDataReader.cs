// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text.Json;

namespace ConfigForge.Runtime.JsonReader;

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
    public string Code => Kind switch
    {
        RuntimeDataErrorKind.Io => "CF_RT_IO",
        RuntimeDataErrorKind.Decode => "CF_RT_DECODE",
        RuntimeDataErrorKind.SchemaMismatch => "CF_RT_SCHEMA_MISMATCH",
        RuntimeDataErrorKind.TypeMismatch => "CF_RT_TYPE_MISMATCH",
        RuntimeDataErrorKind.NotFound => "CF_RT_NOT_FOUND",
        RuntimeDataErrorKind.Unsupported => "CF_RT_UNSUPPORTED",
        RuntimeDataErrorKind.InvalidPath => "CF_RT_INVALID_PATH",
        _ => "CF_RT_INTERNAL",
    };
    public string? Path { get; init; }
}


public class JsonRuntimeDataFileLoader
{
    private readonly string _root;

    public JsonRuntimeDataFileLoader(string root)
    {
        _root = root ?? throw new ArgumentNullException(nameof(root));
    }

    public int OpenCount { get; private set; }

    public JsonRuntimeDataReader OpenReader(string path)
    {
        OpenCount++;
        var resourcePath = Path.Combine(_root, path + ".json");
        try
        {
            return new JsonRuntimeDataReader(resourcePath);
        }
        catch (IOException error)
        {
            throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.Io, "failed to read runtime data", error) { Path = resourcePath };
        }
        catch (UnauthorizedAccessException error)
        {
            throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.Io, "failed to read runtime data", error) { Path = resourcePath };
        }
        catch (ArgumentException error)
        {
            throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.Io, "failed to read runtime data", error) { Path = resourcePath };
        }
        catch (NotSupportedException error)
        {
            throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.Io, "failed to read runtime data", error) { Path = resourcePath };
        }
    }
}

public class JsonRuntimeDataReader
{
    public const string TemplateVersion = "0.1.0";
    public const string SupportedConfigForgeReaderContract = "^1.0";
    public const string DistributionMode = "copyable-template";

    private readonly JsonDocument _document;
    private readonly JsonDocument _nullDocument = JsonDocument.Parse("null");
    private readonly Stack<JsonElement> _stack = new();

    public JsonRuntimeDataReader(string path)
        : this(JsonDocument.Parse(File.ReadAllText(path)))
    {
    }

    public JsonRuntimeDataReader(JsonDocument document)
    {
        _document = document ?? throw new ArgumentNullException(nameof(document));
        _stack.Push(_document.RootElement);
    }

    public void EnterRequiredField(string name)
    {
        var current = _stack.Peek();
        if (current.ValueKind == JsonValueKind.Object
            && current.TryGetProperty(name, out var value)
            && value.ValueKind != JsonValueKind.Null)
        {
            _stack.Push(value);
            return;
        }
        throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.NotFound, $"missing required field {name}");
    }

    public bool EnterOptionalField(string name)
    {
        var current = _stack.Peek();
        if (current.ValueKind == JsonValueKind.Object
            && current.TryGetProperty(name, out var value)
            && value.ValueKind != JsonValueKind.Null)
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
        if (current.ValueKind == JsonValueKind.Array)
        {
            return current.GetArrayLength();
        }
        return current.ValueKind == JsonValueKind.Object
            && current.TryGetProperty("rows", out var rows)
            && rows.ValueKind == JsonValueKind.Array
                ? rows.GetArrayLength()
                : 1;
    }

    public void EnterRow(int index)
    {
        var rows = _stack.Peek().GetProperty("rows");
        _stack.Push(rows[index]);
    }

    public void ExitRow() => _stack.Pop();

    public int ReadListLength()
    {
        var current = _stack.Peek();
        if (current.ValueKind != JsonValueKind.Array)
        {
            throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.TypeMismatch, $"expected JSON array, got {current.ValueKind}");
        }
        return current.GetArrayLength();
    }

    public void EnterListElement(int index) => _stack.Push(_stack.Peek()[index]);

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
        return current.ValueKind switch
        {
            JsonValueKind.String => long.Parse(current.GetString() ?? "0", CultureInfo.InvariantCulture),
            JsonValueKind.Number => current.GetInt64(),
            _ => throw new InvalidOperationException($"expected int-like value, got {current.ValueKind}"),
        };
    }

    public double ReadFloat()
    {
        var current = _stack.Peek();
        var value = current.ValueKind switch
        {
            JsonValueKind.String => double.Parse(current.GetString() ?? "0", CultureInfo.InvariantCulture),
            JsonValueKind.Number => current.GetDouble(),
            _ => throw new InvalidOperationException($"expected finite float-like value, got {current.ValueKind}"),
        };
        if (!double.IsFinite(value))
        {
            throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.TypeMismatch, "expected finite float-like value");
        }
        return value;
    }

    public bool ReadBool()
    {
        var current = _stack.Peek();
        return current.ValueKind switch
        {
            JsonValueKind.String => ParseBoolString(current.GetString() ?? "false"),
            JsonValueKind.True => true,
            JsonValueKind.False => false,
            _ => throw new InvalidOperationException($"expected bool-like value, got {current.ValueKind}"),
        };
    }

    public string ReadString()
    {
        var current = _stack.Peek();
        return current.ValueKind switch
        {
            JsonValueKind.String => current.GetString() ?? string.Empty,
            _ => throw new ConfigForgeRuntimeException(RuntimeDataErrorKind.TypeMismatch, $"expected string value, got {current.ValueKind}"),
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
}
