# XLSX 测试与可执行示例

从仓库根目录运行：

```bash
cargo run -- --project tests/fixtures/xlsx/ConfigForge.toml --target client
```

输出位于本目录 `out/`（已忽略），包含 CollectionTable `items` 和 ObjectTable `settings`。

`sources.xlsx` 有意包含多个正常和错误 sheet；示例只选择正常 sheet。读取器单测和 CLI 测试按需选择中文名称、偏移 header、空行列、shared strings、前导零、布尔/数字、公式缓存（含有效空字符串与缺失缓存）、错误值、ISO/序列日期、ObjectTable 和覆盖来源。

`single.xlsx` 验证单 sheet 省略名称。二进制文件是有意提交的输入 fixture，不依赖用户安装 Excel。

使用 `node tests/fixtures/xlsx/generate.mjs` 可确定性重建两个工作簿。脚本只用 Node 标准库维护最小 OOXML 文件和 ZIP，便于精确控制普通表格写入库通常会自动修正的异常缓存。Node 不是 XLSX 功能或这些 Rust/CLI 测试的运行依赖。
