# CLI 参数命名与精简

时间：2026-10-30。以下保留实施前的调研与建议，当前行为以 [CLI 设计](../Design/06-export-cli.md#66-cli-总体形式) 为准。

### 6.6.1 参数命名与精简评估（建议，尚未实施）

2026-10-30 核对当前 Cli、ExecutionOptions、报告 task 注入和可执行文件 --help 后形成以下建议。本节不是已实施接口；上表和现有示例仍描述当前 CLI。

#### 外部工具调研

| 工具与官方文档 | 当前文档中的参数 | 对本工具的启示 |
| --- | --- | --- |
| [Cargo build](https://doc.rust-lang.org/cargo/commands/cargo-build.html) | --target-dir 指定构建目录；--message-format 指定诊断格式 | 产物位置与诊断格式分别命名，不共享一个含糊的 output 概念 |
| [CMake](https://cmake.org/cmake/help/latest/manual/cmake.1.html) | --profiling-format 与 --profiling-output 指定辅助性能数据格式和路径 | 辅助输出使用用途前缀，格式与路径分开 |
| [pytest](https://docs.pytest.org/en/stable/how-to/output.html) | --junit-xml=path 写出 JUnit XML 文件 | 名称直接表达报告种类，避免被理解为测试或工程的产物根目录 |
| [ESLint](https://eslint.org/docs/latest/use/command-line-interface) | --format 选择 lint 结果格式；--output-file 写出检查结果 | 当检查结果就是命令的主要输出时，通用 output-file 有明确上下文；不能直接套用到多种产物的构建入口 |

这些工具没有共同的唯一命名标准。ConfigForge 的同一次调用可生成多种代码、数据、文档和报告，因此报告路径应包含 report 前缀。

#### 推荐方案

1. 将 `--output` 改为 **`--report-output <FILE>`**。与现有 report / report-title 形成报告参数组；帮助与错误消息明确是验证报告文件，且不覆盖 `[[targets.exports]].output`。
2. 继续使用 `--report <FORMAT>` 显式选择格式并启用报告，保留当前参数依赖关系。`--report-format` 可作为以后整体统一命名时的候选，本次消歧不必同时扩大重命名范围。`--report-file` 也清楚，但采用 report-output 即已明确用途，并通过 FILE 明确路径类型。
3. 不将 report 改为文件路径后再自动推断格式；这样会引入扩展名识别、未知后缀、显式覆盖等额外规则。保留显式格式与路径的组合更接近当前实现。
4. 不增加全局 `--output-dir`。当前一个 target 可有多个 export task，各自有不同输出目录；一个全局目录覆盖需要另行定义路径重定位规则。
5. 报告仍是当前 build 的附加任务，不为改参数名新增 report 子命令。报告的 target-aware 校验与失败导出语义保持原样。

推荐命令示例（尚不可作为当前 CLI 执行）：

```bash
config-forge --project ConfigForge.toml --target client \
  --report html --report-output out/report/client-validation.html
```

#### 参数取舍

| 当前参数 | 建议 | 理由与边界 |
| --- | --- | --- |
| `--io-concurrency`、`--network-concurrency` | 优先从 CLI 删除 | 当前均无生产调度消费者；实际本地读取由 jobs 控制。未来有真实异步或远端 provider 后再开放已生效的资源选项 |
| `--serial` | 保留便利入口；仅在追求最少参数时考虑删除 | 对当前生产执行，效果与 jobs 1 重叠；意图清楚且实现成本小。当前它还覆盖冲突的显式并发数值，不能未经说明当作完全等价拼写删除 |
| `--report-title` | 保留可选覆盖 | 可以在配置中声明 title，但临时 CI / QA 报告仍可能需要覆盖；默认值可用不表示参数无效。若以后明确只允许配置中定制标题，再删除 CLI 入口 |
| `--trace-tasks` | 保留能力，可低优先级改名为 trace-output | 当前输出阶段级 BuildTrace，并非任务图。用途有效，问题在名称的精确性，不是冗余 |
| `--report` 与报告路径参数 | 保留，路径按上文改名 | 分别负责格式 / 启用与目标文件；与配置 report task 分别服务临时附加报告和固定工程产物 |
| `--diagnostic-format` | 保留 | stderr 的失败诊断覆盖 Project / Meta / Build 等错误；JSON 报告只表达 validation report，二者范围和生命周期不同 |
| `--dry-run`、`--write-mode` | 保留 | 前者执行完整计划而不写 artifact，后者选择实际写入策略；当前 dry-run 优先，两个概念不相互替代 |
| `--project`、`--target`、`--tag`、`--jobs` | 保留 | 分别选择工程 / target、追加本次上下文、控制有效 worker 数，均有生产消费者 |

删除两个保留并发选项不等于删除公开 Rust ExecutionOptions / BuildTrace 字段；库 API 和 trace schema 的兼容性应单独评估。前一轮为保持接口兼容保留这些入口；本次是在 CLI 使用体验层面重新评估其价值。

#### 迁移与实施验证

- 有外部脚本使用旧 output 时，可在一个明确过渡版本保留隐藏的 deprecated alias；文档与 --help 只展示 report-output。若确认没有兼容需求，可直接移除旧拼写。不给该别名新增其他产物语义。
- 删除未生效的并发选项后，应明确报未知参数或提供迁移错误，提示使用 jobs；不保留继续静默接受数值的无效选项。
- 实施时同步 Cli、错误消息、诊断 hint、测试命令、README / Design 示例和 HTML 摘要；历史 DevLogs 不改写。当前参数表仍维持可执行状态，不提前替换成建议参数。
- 回归覆盖帮助信息、报告参数依赖、报告文件与工程目录隔离、validation 失败时仍写报告、三种写入模式、jobs / serial 行为与 JSON stderr。dry-run 不写报告 artifact，但显式 trace 输出仍会写文件，帮助说明需要保留此边界。


## 实施决策

- 使用 --report-output 明确报告文件路径，直接删除旧 --output 拼写，不保留隐藏 alias。
- 删除 CLI 中没有生产调度作用的 --io-concurrency / --network-concurrency；公开 ExecutionOptions API 和 BuildTrace schema 保持兼容。
- 保留 --report、--report-title、--serial、--trace-tasks 等有效入口，不新增子命令、输出目录覆盖或报告扩展名推断。
- 回归覆盖参数依赖、未知旧参数、报告与工程产物路径隔离、四种报告格式、三种写入模式和 serial/jobs 行为。

## 验证记录

- cargo fmt、cargo test --locked、cargo lint 和 cargo clippy --all-targets -- -D warnings 通过。
- 完整测试包含 555 个库单元测试和 67 个 CLI 集成测试；C#、Java 和 TypeScript runtime harness 实际执行通过。C++ / Unreal / Lua 按 quick profile 的缺工具或依赖策略跳过。
- cargo llvm-cov 对 library + CLI integration 生成 HTML 报告；src/cli.rs 行覆盖率为 97.34%，已检查改动涉及的报告成功/失败路径与参数依赖。
- 改动文档的相对文件链接检查通过。
