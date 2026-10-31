// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

// Deterministic, dependency-free OOXML fixtures, including formula-cache edge cases.
// Run: node tests/fixtures/xlsx/generate.mjs
import { writeFileSync } from 'node:fs';

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function zip(entries) {
  const chunks = [], directory = [];
  let offset = 0;
  for (const [path, content] of entries) {
    const name = Buffer.from(path), data = Buffer.from(content), crc = crc32(data);
    const local = Buffer.alloc(30), central = Buffer.alloc(46);
    local.writeUInt32LE(0x04034b50); local.writeUInt16LE(20, 4);
    local.writeUInt32LE(crc, 14); local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22); local.writeUInt16LE(name.length, 26);
    central.writeUInt32LE(0x02014b50); central.writeUInt16LE(20, 4); central.writeUInt16LE(20, 6);
    central.writeUInt32LE(crc, 16); central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(data.length, 24); central.writeUInt16LE(name.length, 28);
    central.writeUInt32LE(offset, 42);
    chunks.push(local, name, data); directory.push(central, name);
    offset += local.length + name.length + data.length;
  }
  const index = Buffer.concat(directory), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50); end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10); end.writeUInt32LE(index.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...chunks, index, end]);
}

const escape = value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;');
const text = (pos, value) => `<c r="${pos}" t="inlineStr"><is><t>${escape(value)}</t></is></c>`;
const value = (pos, number, type = '') => `<c r="${pos}"${type ? ` t="${type}"` : ''}><v>${number}</v></c>`;
const formula = (pos, expression, cache = undefined, type = '') => `<c r="${pos}"${type ? ` t="${type}"` : ''}><f>${escape(expression)}</f>${cache === undefined ? '' : `<v>${cache}</v>`}</c>`;
const row = (number, ...cells) => `<row r="${number}">${cells.join('')}</row>`;
const worksheet = rows => `<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>${rows.join('')}</sheetData></worksheet>`;

function workbook(filename, sheets) {
  const ns = 'http://schemas.openxmlformats.org';
  const entries = [
    ['[Content_Types].xml', `<Types xmlns="${ns}/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>${sheets.map((_, i) => `<Override PartName="/xl/worksheets/sheet${i + 1}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>`).join('')}</Types>`],
    ['_rels/.rels', `<Relationships xmlns="${ns}/package/2006/relationships"><Relationship Id="rId1" Type="${ns}/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>`],
    ['xl/workbook.xml', `<workbook xmlns="${ns}/spreadsheetml/2006/main" xmlns:r="${ns}/officeDocument/2006/relationships"><sheets>${sheets.map(([name], i) => `<sheet name="${escape(name)}" sheetId="${i + 1}" r:id="rId${i + 1}"/>`).join('')}</sheets></workbook>`],
    ['xl/_rels/workbook.xml.rels', `<Relationships xmlns="${ns}/package/2006/relationships">${sheets.map((_, i) => `<Relationship Id="rId${i + 1}" Type="${ns}/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet${i + 1}.xml"/>`).join('')}<Relationship Id="strings" Type="${ns}/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/><Relationship Id="styles" Type="${ns}/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>`],
    ['xl/sharedStrings.xml', `<sst xmlns="${ns}/spreadsheetml/2006/main" count="1" uniqueCount="1"><si><t>药水</t></si></sst>`],
    ['xl/styles.xml', `<styleSheet xmlns="${ns}/spreadsheetml/2006/main"><cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="14" applyNumberFormat="1"/></cellXfs></styleSheet>`],
    ...sheets.map(([, rows], i) => [`xl/worksheets/sheet${i + 1}.xml`, worksheet(rows)]),
  ];
  writeFileSync(new URL(filename, import.meta.url), zip(entries));
}

const items = [
  row(3, text('B3', 'id'), text('D3', 'name'), text('E3', 'enabled'), text('F3', 'amount'), text('G3', 'computed'), text('H3', 'empty')),
  row(4, text('B4', 'title'), text('D4', '名称')),
  row(5, value('B5', 1), text('D5', '0012'), value('E5', 1, 'b'), value('F5', 12.5), formula('G5', '1+2', '3'), formula('H5', '""', '', 'str'), value('J5', '#DIV/0!', 'e')),
  row(6, text('B6', ''), text('D6', '')),
  row(7, value('B7', 2), value('D7', '0', 's'), value('E7', 0, 'b'), value('F7', 20), value('G7', 4)),
];
workbook('sources.xlsx', [
  ['物品', items],
  ['settings', [row(2, text('B2', 'key'), text('D2', 'value')), row(4, text('B4', 'limit'), value('D4', 10)), row(6, text('B6', 'title'), text('D6', '中文标题'))]],
  ['override', [row(1, text('A1', 'id'), text('B1', 'name')), row(3, value('A3', 1), text('B3', 'updated'))]],
  ['missing_cache', [row(1, text('A1', 'value')), row(3, formula('A3', '1+2'))]],
  ['error', [row(1, text('A1', 'value')), row(3, value('A3', '#DIV/0!', 'e'))]],
  ['date', [row(1, text('A1', 'value')), row(3, value('A3', '2026-01-01T00:00:00', 'd'))]],
  ['serial_date', [row(1, text('A1', 'value')), row(3, '<c r="A3" s="1"><v>46023</v></c>')]],
  ['duration', [row(1, text('A1', 'value')), row(3, value('A3', 'PT1H', 'd'))]],
  ['titled_object', [row(1, text('B1', 'key'), text('D1', 'title'), text('F1', 'value')), row(2, text('B2', 'x'), formula('D2', '1+2'), value('F2', 10))]],
  ['empty_key', [row(1, text('A1', 'key'), text('B1', 'value')), row(2, value('B2', 10))]],
  ['duplicate', [row(2, text('B2', 'id'), text('D2', 'id'))]],
  ['invalid', [row(3, text('B3', 'id')), row(5, text('B5', 'wrong'))]],
  ['empty', []],
  ['object_duplicate', [row(1, text('A1', 'key'), text('B1', 'value')), row(2, text('A2', 'x'), value('B2', 1)), row(3, text('A3', 'x'), value('B3', 2))]],
]);
workbook('single.xlsx', [['only', [row(1, text('A1', 'id')), row(3, value('A3', 1))]]]);
