// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

using System;
using System.Collections.Generic;
using System.Text.Json;
using ConfigForge.Runtime.JsonReader;

namespace ConfigForge.Runtime.SystemTextJsonReader;

public class SystemTextJsonRuntimeDataFileLoader
{
    private readonly JsonRuntimeDataFileLoader _loader;

    public SystemTextJsonRuntimeDataFileLoader(string root)
    {
        _loader = new JsonRuntimeDataFileLoader(root);
    }

    public int OpenCount => _loader.OpenCount;

    public SystemTextJsonRuntimeDataReader OpenReader(string path)
    {
        return new SystemTextJsonRuntimeDataReader(_loader.OpenReader(path));
    }
}

public class SystemTextJsonRuntimeDataReader
{
    public const string TemplateVersion = "0.1.0";
    public const string SupportedConfigForgeReaderContract = "^1.0";
    public const string DistributionMode = "copyable-template";
    public const string ParserBackend = "System.Text.Json";
    public const string ParserBackendVersion = "net6.0";
    public const string AdapterSupportLevel = "stable";

    private readonly JsonRuntimeDataReader _reader;

    public SystemTextJsonRuntimeDataReader(string path)
        : this(new JsonRuntimeDataReader(path))
    {
    }

    public SystemTextJsonRuntimeDataReader(JsonDocument document)
        : this(new JsonRuntimeDataReader(document))
    {
    }

    public SystemTextJsonRuntimeDataReader(JsonRuntimeDataReader reader)
    {
        _reader = reader ?? throw new ArgumentNullException(nameof(reader));
    }

    public void EnterRequiredField(string name) => _reader.EnterRequiredField(name);
    public bool EnterOptionalField(string name) => _reader.EnterOptionalField(name);
    public void ExitField() => _reader.ExitField();
    public ulong? SchemaRevision() => _reader.SchemaRevision();
    public int ReadRowCount() => _reader.ReadRowCount();
    public void EnterRow(int index) => _reader.EnterRow(index);
    public void ExitRow() => _reader.ExitRow();
    public int ReadListLength() => _reader.ReadListLength();
    public void EnterListElement(int index) => _reader.EnterListElement(index);
    public void ExitListElement() => _reader.ExitListElement();
    public void BeginRecord(int optionalSlotCount) => _reader.BeginRecord(optionalSlotCount);
    public void EndRecord() => _reader.EndRecord();
    public int ReadEnumIndex(IReadOnlyList<string> variants) => _reader.ReadEnumIndex(variants);
    public void EnsureExhausted() => _reader.EnsureExhausted();
    public long ReadInt() => _reader.ReadInt();
    public double ReadFloat() => _reader.ReadFloat();
    public bool ReadBool() => _reader.ReadBool();
    public string ReadString() => _reader.ReadString();
}
