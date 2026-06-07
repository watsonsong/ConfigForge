# Designer Validation Report 实现 TODO

本文把 [`Design/10-designer-validation-report.md`](../Design/10-designer-validation-report.md) 中的 P1-1 Designer Validation Report 设计拆成可独立开发、验证和提交的实现步骤。当前实现基线是：`src/diagnostic/` 已提供通用诊断模型，`src/validation/` 已提供结构化 `ValidationReport` / `ValidationIssue`、target-aware `ExportValidationView` 和 validator registry；但还没有面向策划 / QA / CI 的报告模型、报告 renderer、CLI 报告导出参数或 build 附加产物。

## 总体原则

- 每个 Step 必须能独立编译、独立测试、独立提交。
- 报告导出只消费已有 validation 结果，不重复执行另一套校验逻辑。
- `ValidationIssue` / `Diagnostic` 保持程序稳定契约；面向人的标题、解释、修复建议、owner/domain 聚合应放在 report enrichment 层。
- HTML / Markdown / JSON / CSV 四种输出必须共享同一个 normalized report model，避免各 renderer 自己解释诊断。
- 报告内容必须遵循 Full DataSet / ExportView 语义；target-aware validate/build 输出的报告只描述本次 target 视图下的校验结果。
- 即使 validation 存在 error，报告文件也应尽量写出；报告写入失败不得掩盖原始 validation 失败。
- 输出顺序必须 deterministic：severity、table、source path、source row、field、code、message 等排序规则需固定并覆盖测试。
- 新增 CLI / Project Config 字段必须显式拒绝未知格式或不支持组合，不能静默忽略。

## [ ] Step 1：建立 DesignerValidationReport 核心模型与 enrichment builder

目标：引入报告层 normalized model，把现有 `ValidationReport` 转成稳定、可测试、可被多 renderer 复用的数据结构。

范围：

- 在合适模块下新增报告子模块，例如 `src/validation/report/` 或独立 `src/report/`，并保持依赖方向不反向污染 validator 规则。
- 定义 `DesignerValidationReport`、`ReportMetadata`、`ReportSummary`、`DesignerValidationIssue`、`IssueGroup` 等核心结构。
- 为每条 issue 生成稳定展示 ID，例如 `E0001` / `W0001`。
- 从 Project / target / `ValidationReport` / issue location 中填充 metadata、summary、severity、code、message、table、field、row index、source row、target tags 等已有信息。
- owner/domain 先提供 fallback：`<unassigned>` / `<uncategorized>`；暂不修改 Meta schema。
- 明确缺失字段的表现：`source_column`、`current_value`、`expected` 等无法获取时用 `None` / 空字段，而不是编造值。

验收：

- 单元测试覆盖空报告、只有 warning、同时有 error/warning、多表多字段、多 target tags。
- 单元测试覆盖稳定 ID 和 deterministic issue 排序。
- 不新增 CLI 参数，不写任何文件产物。

建议提交：`feat(validation): add designer report model`

## [ ] Step 2：建立 diagnostic code catalog 与内置 validator 解释映射

目标：为人工报告补充 `friendly_title`、`explanation`、`hint` 和可选 `expected` 文案，使报告不只展示底层 message。

范围：

- 新增 code catalog 结构，例如 `DiagnosticCodeCatalog` / `ReportCodeInfo`。
- 覆盖当前已实现内置 validator codes：`range`、`regex`、`string_length`、`required_when`、`compare`、`sum`、`monotonic`。
- 覆盖基础 validation 高频 code（如 required/type/enum/unique/FK/asset）中当前已有稳定 code；没有稳定 code 的保持 fallback。
- catalog 只做展示增强，不改变 validator compile / validate 行为。
- enrichment builder 接入 catalog，优先使用 issue 自带 hint，再用 catalog hint 兜底。

验收：

- 单元测试覆盖已知 code 命中 catalog、未知 code fallback message、hint 优先级。
- catalog 文案为中文主描述，字段名 / code 保持英文稳定符号。
- 不新增 renderer 和 CLI 参数。

建议提交：`feat(validation): enrich report issues with code catalog`

## [ ] Step 3：实现 JSON renderer

目标：先落地最容易稳定测试、最适合平台消费的机器可读输出，作为后续 renderer 的 golden contract。

范围：

- 为 `DesignerValidationReport` 增加 serde 序列化结构；如果内部结构不适合直接导出，可定义 `SerializableDesignerValidationReport`。
- JSON 字段使用 snake_case，与设计文档字段保持一致。
- 输出包含 `metadata`、`summary`、`groups`、`issues`。
- 空字段使用 `null` 或空数组，策略需固定并测试。
- 输出 pretty JSON，便于 diff 和人工检查。

验收：

- 单元测试或 snapshot 覆盖 JSON 字段、排序、空报告、error/warning 混合报告。
- JSON round-trip 或 schema-like 测试确认关键字段存在。
- 不接入 CLI；renderer 可由测试直接调用。

建议提交：`feat(validation): render designer report as json`

## [ ] Step 4：实现 Markdown renderer

目标：支持 MR comment、CI 日志和纯文本归档场景。

范围：

- 新增 Markdown renderer，共享 Step 1 的 normalized report model。
- 输出顶部结论：报告标题、target、target tags、BLOCKED / PASS WITH WARNINGS / PASS、error/warning 数量。
- 输出 Top tables / Top codes / owner-domain 摘要。
- 输出 issue 明细表，包含 ID、severity、code、table、field、source row、message、hint。
- 对 Markdown 特殊字符做转义，避免表格被用户数据破坏。

验收：

- 单元测试覆盖无 issue、warning-only、blocked error、多行 message/hint、Markdown 表格转义。
- 输出顺序 deterministic。
- 不接入 CLI；renderer 可由测试直接调用。

建议提交：`feat(validation): render designer report as markdown`

## [ ] Step 5：实现 CSV renderer

目标：支持 QA 把 issue 导入 Excel / Sheet 进行筛选、分派和留档。

范围：

- 新增 CSV renderer，一行一个 issue。
- 字段至少包括：`id`、`severity`、`blocking`、`code`、`friendly_title`、`message`、`hint`、`table`、`field`、`row_index`、`source_row_number`、`source_path`、`target`、`owner`、`domain`。
- 使用稳定 header 顺序。
- 正确转义逗号、引号、换行和空值。
- P1-1 默认单文件 CSV；多文件 CSV 可后续扩展。

验收：

- 单元测试覆盖 header 顺序、空报告、特殊字符转义、可被标准 CSV reader 读回。
- 输出顺序与 JSON / Markdown 一致。
- 不接入 CLI；renderer 可由测试直接调用。

建议提交：`feat(validation): render designer report as csv`

## [ ] Step 6：实现 HTML renderer

目标：提供策划 / QA 最主要的人类阅读格式，支持离线打开、无外部依赖、自包含样式。

范围：

- 新增 HTML renderer，输出完整 HTML 文档。
- 自包含 CSS，不依赖 CDN、JS 框架或外部图片。
- 顶部展示状态卡片：BLOCKED / PASS WITH WARNINGS / PASS、error/warning 数量、target、生成时间。
- 提供按 severity、table、owner/domain、code 的阅读区块或摘要表。
- issue 明细包含源文件、源行号、字段、当前值（如有）、期望（如有）、解释和修复建议。
- 对所有用户数据做 HTML escape。

验收：

- 单元测试覆盖 HTML escape、空报告、warning-only、blocked error、分组区块存在。
- HTML 输出不包含外部 URL 依赖。
- 不接入 CLI；renderer 可由测试直接调用。

建议提交：`feat(validation): render designer report as html`

## [ ] Step 7：接入 `validate` 命令报告导出参数

目标：让用户可以通过 CLI 直接导出报告文件。

范围：

- 为 `validate` 命令增加 `--report <html|markdown|json|csv>`、`--output <path>`、可选 `--report-title <title>`。
- `--report` 未设置时保持现有行为不变。
- `--report` 设置但缺少 `--output` 时给出明确 CLI 配置错误。
- validate 执行后基于同一个 `ValidationReport` 构建 `DesignerValidationReport` 并调用对应 renderer 写文件。
- validation 有 error 时：尽量先写报告，再保持失败退出码。
- 报告写入失败时：输出额外诊断或错误信息，不吞掉原始 validation 信息。

验收：

- CLI integration tests 覆盖四种 format 的文件生成。
- 覆盖 validation failed 但报告仍生成的路径。
- 覆盖未知 format、缺 output、父目录不存在或不可写等错误路径。
- 现有 validate 行为测试不回退。

建议提交：`feat(cli): export validation reports from validate`

## [ ] Step 8：接入 build 附加产物 `validation_report`

目标：让报告可以作为 target build 的附加 artifact 输出，服务 CI 和构建归档。

范围：

- 在 target export task 配置中支持 `type = "validation_report"`、`format`、`output`、`file_name`、`include_warnings`、`include_errors` 等字段。
- 将 `validation_report` 作为聚合 export task 接入 build orchestration，消费当前 target validation result。
- validation error 阻塞 runtime data/codegen 时，`validation_report` 仍应尽量写出。
- 明确与现有 `targets.data` / `targets.code` / aggregate exporter 的关系；必要时先实现为 build 阶段特殊 artifact，而不是混入 runtime data exporter。
- 输出路径纳入 artifact path conflict 检查。

验收：

- Project config parse/link tests 覆盖合法配置、未知 format、缺 output、非法 file_name。
- Build integration tests 覆盖成功 build 生成报告、validation failed 仍生成报告、路径冲突报错。
- 不改变 runtime data/codegen 既有产物内容。

建议提交：`feat(build): add validation report export task`

## [ ] Step 9：补充 owner/domain 配置来源

目标：让报告可以按负责人和玩法域分派问题，同时避免 P1-1 初期强行修改所有 Meta。

范围：

- 先支持 Project 级可选映射，例如 `[validation_report.owners]`、`[validation_report.domains]` 或等价结构。
- 映射粒度至少支持 table；field 级映射可用 `table.field` key 或后续扩展。
- enrichment builder 按 table / field 填充 owner/domain，缺失时 fallback。
- 保持 Meta schema owner/domain 扩展为后续可选步骤；如本 Step 决定同时支持 Meta 字段，必须同步更新 Design / Implementation 文档。

验收：

- 配置解析测试覆盖 table owner、field owner、缺失 fallback、未知 table/field 映射的处理策略。
- 报告分组测试覆盖 owner/domain Top N 和 issue 字段填充。
- CLI / build 报告均能读取同一映射。

建议提交：`feat(validation): group designer reports by owner domain`

## [ ] Step 10：补齐端到端 fixtures、文档和回归测试

目标：把 Designer Validation Report 从功能闭环推进到可维护状态。

范围：

- 在 `tests/fixtures/standard_project/` 或专用 fixture 中构造稳定的 error/warning 样例。
- 为 validate CLI 和 build task 增加端到端测试，覆盖至少 JSON + HTML；Markdown / CSV 可用 renderer 单测和轻量 CLI 测试覆盖。
- 更新用户面文档：Design / README / HTML 阅读版 / CLI 示例中涉及的报告能力状态。
- 如新增模块边界或 build 配置结构，更新对应 Implementation 文档。
- 确认 `cargo fmt`、相关单元测试、CLI integration tests、必要的 `cargo test` 通过。

验收：

- 报告功能有稳定 fixture 和 golden/snapshot 策略。
- 文档状态从 Planned / Not implemented 更新为 Implemented MVP 或 Partial，并准确列出边界。
- 全量测试通过。

建议提交：`test(validation): cover designer report end to end`
