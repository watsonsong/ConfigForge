// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text.Json;

namespace ConfigForge.Runtime.JsonReader;

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
        return new JsonRuntimeDataReader(Path.Combine(_root, path + ".json"));
    }
}

public class JsonRuntimeDataReader
{
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
        if (current.ValueKind == JsonValueKind.Object && current.TryGetProperty(name, out var value))
        {
            _stack.Push(value);
            return;
        }
        _stack.Push(_nullDocument.RootElement);
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

    public int ReadRowCount()
    {
        var current = _stack.Peek();
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
            throw new InvalidOperationException($"expected JSON array, got {current.ValueKind}");
        }
        return current.GetArrayLength();
    }

    public void EnterListElement(int index) => _stack.Push(_stack.Peek()[index]);

    public void ExitListElement() => _stack.Pop();

    public long ReadInt()
    {
        var current = _stack.Peek();
        return current.ValueKind switch
        {
            JsonValueKind.String => long.Parse(current.GetString() ?? "0", CultureInfo.InvariantCulture),
            JsonValueKind.Number => current.GetInt64(),
            _ => 0,
        };
    }

    public double ReadFloat()
    {
        var current = _stack.Peek();
        return current.ValueKind switch
        {
            JsonValueKind.String => double.Parse(current.GetString() ?? "0", CultureInfo.InvariantCulture),
            JsonValueKind.Number => current.GetDouble(),
            _ => 0.0,
        };
    }

    public bool ReadBool()
    {
        var current = _stack.Peek();
        return current.ValueKind switch
        {
            JsonValueKind.String => ParseBoolString(current.GetString() ?? "false"),
            JsonValueKind.Number => current.GetInt64() != 0,
            JsonValueKind.True => true,
            JsonValueKind.False => false,
            _ => false,
        };
    }

    public string ReadString()
    {
        var current = _stack.Peek();
        return current.ValueKind switch
        {
            JsonValueKind.String => current.GetString() ?? string.Empty,
            JsonValueKind.Number => current.GetRawText(),
            JsonValueKind.True => "true",
            JsonValueKind.False => "false",
            _ => string.Empty,
        };
    }

    private static bool ParseBoolString(string value)
    {
        return value.Trim() switch
        {
            "1" => true,
            "0" => false,
            var text when bool.TryParse(text, out var parsed) => parsed,
            _ => throw new FormatException($"invalid boolean string: {value}"),
        };
    }
}
