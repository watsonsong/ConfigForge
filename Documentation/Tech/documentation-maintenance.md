# 文档维护与 Design HTML

## 职责与阅读入口

| 文档 | 面向读者与职责 |
| --- | --- |
| [根 README](../../README.md)、[中文 README](../../README.zh-CN.md) | 用户入口：产品能力、可运行的示例和文档链接；两种语言保持相同功能范围。 |
| [AGENTS.md](../../AGENTS.md) | Agent 的简短入口、选文档规则和工作约束，不复制完整文档目录。 |
| [Documentation/README.md](../README.md) | 唯一完整文档地图与维护规则。 |
| [Design/README.md](../Design/README.md) | 当前产品决策、状态矩阵与专题索引；专题描述当前契约和明确的未来边界。 |
| [Implementation 大纲](../Implementation/01-module-outline.md) | 当前模块、单向依赖、接口职责与实现专题。 |
| Tech | 工具链、三方依赖、测试/CI 和工程实践。 |
| [DevLogs](../DevLogs/README.md) | 讨论证据、历史方案、评审/coverage 快照和临时实施清单，不作为当前契约。 |

一个主题选择一个权威位置，其他文档用链接引用。新增术语同步 [glossary](../glossary.md)；当前支持、部分支持和未来能力采用 Design 状态词。不要把已完成 Phase/Step 计划、无日期测试数字、失效 Todo 链接保留在当前设计正文；保留必要历史时写入 DevLogs，当前文档只留下现状和真实限制。

## Design HTML 生成

[`Design/html/index.html`](../Design/html/index.html) 是 Design README 和 `01`～`11` Markdown 的完整静态阅读版，不是另一份手工摘要，也不是 `schema_doc` generator 的输出。Markdown 是唯一编辑源。

在仓库根目录执行：

```bash
npm ci --prefix tools/docs --include=dev
npm run docs:build --prefix tools/docs
npm run docs:check --prefix tools/docs
```

维护工具要求 Node.js 20+，使用 lockfile 固定的 Marked；不会修改 Rust/runtime 依赖。首次安装需要 npm 网络访问，安装后生成与检查均可离线执行。生成器按固定文件顺序渲染，统一 LF，不写时间戳；内部 Design 链接转换为页面锚点，其余相对链接调整到 HTML 所在目录。重复或缺失内部锚点导致失败。

HTML 提供响应式目录、代码块、表格与打印样式，不加载外部 JS/CSS/CDN；离线阅读不要求 Node。不要直接修改 HTML，刷新结果和 Markdown 一起提交。Design 的 fenced text 流程图按原文保留。

## 文档变更验证

1. 核对涉及的 CLI、config、源码符号、fixture 与状态矩阵。当前源码和可执行示例用于确认实现事实，目标设计必须明确标记。
2. 运行 docs:check 检查当前 Markdown 本地文件链接和 heading anchors，忽略 fenced code 中表示生成产物的示例链接；检查新增文档是否已进入总索引及相关入口。
3. README 命令使用真实 fixture 验证；配置片段要注明是否完整可执行。不要把不存在的目录或尚未编写的文档当作入口。
4. Design 变化运行上述 build/check，并检查 HTML 的桌面/窄屏阅读、目录锚点和表格溢出。
5. 纯文档不需要重复运行无关的完整 Rust/runtime 测试；涉及代码、配置或 fixture 行为时仍遵守 [代码规范](../code-style.md)和 [测试矩阵](test-ci-matrix.md)。

仓库内历史文件名 `08-exporter-*` / `09-codegen-*` 为链接稳定保留，正文分别使用 Data Artifact Toolkit / Code Artifact Toolkit；具体生成器在 `src/generator/<type_id>/`。不因术语收敛反复重命名专题文件。
