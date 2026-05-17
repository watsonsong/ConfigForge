# 单元测试覆盖率评估

本文档记录 ConfigForge 项目的 Rust 单元测试覆盖率评估方法。覆盖率不是单元测试完备性的唯一标准，但必须作为每次增加功能和测试代码后判断测试是否充分的重要设计依据。

## 1. 目标

覆盖率评估用于回答三个问题：

1. 新增功能的正常路径是否被测试执行到。
2. 关键异常路径、错误返回、诊断分支是否被测试执行到。
3. 未覆盖代码是否属于明确可接受的暂缓范围，还是需要补充测试。

因此，每次完成一项功能或修改测试后，至少应运行一次覆盖率评估，并结合 HTML 报告检查新增或改动模块的未覆盖行。

## 2. 工具选择

项目使用 `cargo-llvm-cov` 作为覆盖率工具。

原因：

- 基于 LLVM source-based coverage，适合 Rust 项目。
- 支持终端汇总、HTML 报告、LCOV 输出。
- 在 macOS 上比 `cargo-tarpaulin` 更稳定。

当前项目已经在 `.cargo/config.toml` 中配置了 Cargo alias：

```toml
[alias]
coverage = "llvm-cov --workspace --all-features --summary-only"
coverage-html = "llvm-cov --workspace --all-features --html"
coverage-lcov = "llvm-cov --workspace --all-features --lcov --output-path target/llvm-cov/lcov.info"
```

## 3. 环境安装

首次使用前安装 LLVM coverage 组件：

```bash
rustup component add llvm-tools-preview
```

安装 `cargo-llvm-cov`：

```bash
cargo install cargo-llvm-cov --locked
```

如果本机 Rust/Cargo 版本较旧，最新 `cargo-llvm-cov` 可能无法编译。当前项目已验证可在旧工具链上安装：

```bash
cargo install cargo-llvm-cov --version 0.4.14 --locked
```

安装完成后检查：

```bash
cargo llvm-cov --version
```

## 4. 运行方式

### 4.1 终端汇总

```bash
cargo coverage
```

输出每个源文件的 region、function、line 覆盖率，以及总覆盖率。日常开发中应先看这个汇总，确认改动模块是否明显低于预期。

### 4.2 HTML 报告

```bash
cargo coverage-html
```

报告生成路径：

```text
target/llvm-cov/html/index.html
```

HTML 报告用于逐行检查未覆盖代码。评估单元测试是否完备时，必须打开与本次改动相关的文件，确认未覆盖行是否合理。

### 4.3 LCOV 输出

```bash
cargo coverage-lcov
```

输出路径：

```text
target/llvm-cov/lcov.info
```

该文件主要用于 CI 或外部覆盖率平台接入。

## 5. 评估口径

覆盖率评估不只看总百分比，更要看改动模块的分支覆盖情况。

### 5.1 正常路径

新增功能至少应覆盖：

- canonical 输入能成功解析或执行。
- 主要 public API 正常返回。
- 典型组合场景能串联通过。
- 新增 linked/reference 结构能被断言到真实目标对象。

示例：Meta 模块新增 Link 能力时，正常路径测试应断言 raw 字符串已经解析到对应 `Linked*` 引用。

### 5.2 异常路径

新增功能还应覆盖：

- 非法输入。
- 缺失引用。
- 重复命名。
- I/O 错误。
- parse 错误。
- validation 错误。
- 冲突配置。

异常测试不一定追求所有错误文本完全匹配，但应至少断言稳定的错误类型、diagnostic code 或关键 message。

### 5.3 可接受的未覆盖代码

以下代码可以暂时不作为补测阻塞项，但应在评估时明确说明：

- 单纯的 `Display` / `Debug` 展示路径。
- 需要 OS 权限或特殊文件系统状态才能触发的错误路径。
- 暂未对外使用的兼容保留字段。
- 未来功能预留但当前未进入执行链路的代码。

如果未覆盖代码属于业务规则、数据转换、Link、validate、export 产物生成，则默认应补充测试。

## 6. 建议阈值

当前阶段不设置硬性全局阈值，但采用以下判断标准：

| 范围 | 建议 |
| --- | --- |
| 新增或重构模块 | 行覆盖率尽量不低于 80%，核心逻辑应接近 90%。 |
| `meta/link`、`meta/validate` 等规则模块 | 正常和异常分支都应覆盖，不能只覆盖成功路径。 |
| I/O 边界模块 | 正常路径必须覆盖，关键错误路径尽量覆盖。 |
| CLI / integration | 至少覆盖成功 workflow；参数错误可逐步补充。 |

如果覆盖率低于建议值，可以接受，但需要在提交说明或评审说明中解释原因和后续补测计划。

## 7. 每次功能开发后的检查流程

开发完成后执行：

```bash
cargo fmt --check
cargo test
cargo coverage
cargo coverage-html
```

检查顺序：

1. `cargo test` 必须通过。
2. `cargo coverage` 查看总览，重点关注本次改动文件。
3. 打开 HTML 报告，逐行查看本次改动代码的未覆盖行。
4. 如果新增分支未覆盖，优先补充单元测试。
5. 如果未覆盖合理，记录原因。

## 8. 当前项目覆盖率基线

最近一次覆盖率运行结果：

```text
TOTAL:
Regions   91.17%
Functions 90.60%
Lines     92.92%
Branches  -
```

重点文件：

| 文件 | 行覆盖率 | 说明 |
| --- | ---: | --- |
| `meta/link.rs` | 93.96% | Link 主流程覆盖较充分，仍有少量异常分支可补。 |
| `meta/validate.rs` | 95.63% | duplicate、primary index、export condition、validator meta 等主要校验路径已覆盖。 |
| `meta/loader.rs` | 93.55% | 正常加载、imports、I/O/parse 异常路径已有覆盖。 |
| `meta/model.rs` | 98.36% | 查询 helper、target tags、row export policy 等模型行为覆盖较充分。 |
| `validation/mod.rs` | 64.63% | 基础 required/type/enum/unique 与 target-aware validation 已覆盖；仍有 helper/边界分支未完全覆盖。 |
| `validation/builtins/range.rs` | 81.31% | range validator 的参数解析、target 过滤、warning/error、空值和边界比较已有覆盖。 |
| `validation/context.rs` | 93.58% | ExportValidationView / ExportTableView 主要路径已覆盖。 |
| `validation/diagnostic.rs` | 71.76% | Error/Warning 报告路径已覆盖，格式化分支仍可补充。 |
| `validation/registry.rs` | 88.46% | registry require、scope 校验、compile/run 主要路径已覆盖。 |

该基线不作为长期目标，只用于后续迭代比较。后续新增 Meta、Validation、Exporter 功能时，应逐步提高核心模块覆盖率。

## 9. 当前已识别的补测方向

优先补充：

1. Meta validate 异常分支：duplicate enum / struct / enum value / index、empty index fields、missing primary index、多 primary index。
2. Loader 异常分支：invalid import TOML、更多 import cycle / canonicalize 边界。
3. Data validation 异常分支：更多 target-aware unique/index 边界、ValidationReport warning 格式化分支。
4. Source / Exporter 错误路径：write output failure、重复 output path、重复 target 与 warning 输出组合场景。

这些补测项应结合后续功能迭代逐步完成。
