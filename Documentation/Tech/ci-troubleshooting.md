# CI 排错与开发注意事项

本文汇总已验证的 CI 失败模式及对应的开发约束。工具安装和测试覆盖范围分别以
[`development-environment.md`](development-environment.md) 与
[`test-ci-matrix.md`](test-ci-matrix.md) 为准；本页集中维护排错方法，避免重复描述环境矩阵。

## 先确认失败阶段

1. 记录 run URL、被测 commit、失败 job / step 和日志中的第一个实际错误。
   `Process completed with exit code 1/101` 只是结果；后续 job 的 `skipped` 通常是前置失败的影响。
2. 区分 workflow 解析、runner 分配、依赖安装、编译、测试启动和测试断言失败。
   不要把 runtime 缺失、测试命令不兼容或并发断言误报一概视为业务代码编译失败。
3. 使用失败 job 的工具版本、工作目录、环境变量和完整命令复现。至少核对
   `rustc --version`、`cargo --version`、`node --version`、`npm --version`、`dotnet --info`；
   同时区分运行某个 GitHub Action 的 Node 版本与 `setup-node` 提供给项目命令的 Node 版本。
4. 修改依赖、package scripts 或构建顺序后，在临时干净检出中执行安装、检查、构建和测试。
   不删除开发者现有工作区来模拟 CI，也不依赖本地 `node_modules`、忽略的 `dist` 或旧产物。
5. 修复后运行对应失败入口，再执行项目要求的 fmt、lint 和完整测试。
   报告哪些语言 harness 实际执行、哪些跳过；本地通过不等于远端矩阵已经通过。

启用 `CONFIG_FORGE_REQUIRE_TOOLS` 后，默认 optional skip 上限也会变为 0。
只要求部分语言工具时，应明确配置本次允许的其他语言 skip 上限并核对 summary；
不能把这类策略失败当成 harness 本身运行失败，更不能放宽必需工具检查来掩盖缺失环境。
Action 自身的 Node 弃用 warning 与项目测试失败分开处理，先修复实际阻断步骤。

## Actions runtime 与 runner 镜像告警

[run 41168923791](https://github.com/watsonsong/ConfigForge/actions/runs/41168923791)
的五个 job 均成功，5 条 warning 是同一类 Node 20 弃用提示：quality job 引用了
`actions/checkout@v4` / `actions/setup-node@v4`，四个构建 job 各引用了 `checkout@v4`。
它们在 `action.yml` 中声明 Node 20，由 GitHub 强制切换到 Node 24；这不是 Rust 编译告警，
也不是项目通过 `setup-node` 安装的 Node 版本错误。

处理方式与维护规则：

- 检查被引用版本的官方 `action.yml` 中 `runs.using`，以及 release notes 的迁移约束。
  项目 `node-version: "24"` 只影响后续 shell 命令，不能替换 Action 自身的 runtime。
  不用屏蔽告警或设置强制 runtime 环境变量代替 Action 升级。
- CI / Release 的 checkout 统一使用 `actions/checkout@v7`，Node 安装使用
  `actions/setup-node@v7`。这两者原生使用 Node 24；当前仅使用普通 checkout、显式 npm cache，
  没有使用 v7 迁移涉及的 fork privileged checkout 或 registry token fallback。
- 同时检查较少运行的 Release：`upload-artifact@v7`、`download-artifact@v8` 和
  `softprops/action-gh-release@v3` 均原生使用 Node 24，避免发布 tag 时才遇到同类告警。
  产物上传显式 `archive: true`，下载显式 `merge-multiple: false`，保留每个 artifact
  一个目录、目录内包含 release archive / checksum 的布局，供后续 SHA256SUMS 和发布步骤使用。
- Node 24 Action 至少要求 Actions Runner 2.327.1；checkout 的跨 Docker 认证还要求
  2.329.0。该 run 的 hosted runner 为 2.338.2，满足要求；未来使用 self-hosted runner
  时须先核实 runner 版本。Action major、项目 Node major 和 runner agent 是三个独立版本。
- 该 run 另外有 **3 条 notice**：两个 Ubuntu job 的镜像迁移通知，以及 macOS ARM64
  容量通知。它们不是上述 5 条 warning，也不是失败信号。
  Linux job 固定 `ubuntu-24.04`，延续该次成功运行实际使用的 Ubuntu 24.04，避免
  `ubuntu-latest` 自动迁移至 Ubuntu 26 改变系统库和预装工具。固定 OS label 不会冻结
  镜像的每周软件更新；升级 OS 时仍要运行整个目标矩阵，并同步 CI / Release。
- macOS ARM64 排队容量由 GitHub 管理；不通过删除 ARM64 job、替换 Intel 架构或关闭检查
  消除提示。只有持续影响交付时，才评估服务容量或 runner 方案。

workflow 修改先运行 actionlint，并核对 Action 输入及 artifact 布局的兼容性。
Action runtime、缓存服务、上传下载和 hosted image 的最终接入需由 push 后的 CI / tag workflow
验证；已有 run 的告警是历史记录，不会因本地修改消失。

升级时参考官方 [checkout 迁移说明](https://github.com/actions/checkout/tree/v7)、
[setup-node 迁移说明](https://github.com/actions/setup-node/tree/v7)、
[upload-artifact](https://github.com/actions/upload-artifact/tree/v7)、
[download-artifact](https://github.com/actions/download-artifact/tree/v8) 与
[Release Action](https://github.com/softprops/action-gh-release/tree/v3)，
不要仅根据 major 数字推断 runtime 或输入兼容性。

## 已验证的失败模式与预防规则

| 症状 | 原因 | 后续开发注意事项 |
| --- | --- | --- |
| Node 24 下 `MODULE_NOT_FOUND`，路径指向 `runtime/typescript/tests`。 | `node --test tests` 将目录当作模块入口；旧版 Node 本地测试可以通过。 | workspace 使用 `node --test` 的默认发现方式，测试命名为 `*.test.cjs`；从 workspace 根目录运行 `npm test`。修改测试入口时确认实际发现的测试数量，不能只看退出码。用 CI 的 Node major 验证完整 npm script，不以旧版 Node 下直接调用测试文件代替。 |
| C# 已编译成功，执行时报缺少 `Microsoft.NETCore.App`。 | 安装的 SDK 能编译较旧的 target framework，但没有对应 major 的 runtime。 | harness 的 `net8.0` 与 CI 安装的 .NET 8 SDK/runtime 同步；用 `dotnet --list-runtimes` 核实，不能只检查 `dotnet --version`。 |
| TypeScript 报 `TS2339`，例如 `RarityLevelsRow` 没有 `nameText`。 | harness 调用了不存在的 generated API；文本 helper 是模块级函数。 | 以实际生成的 API 契约为准，修改模板、harness 和文档时同步验证。运行真实生成、编译和执行链路，字符串断言不能替代它。 |
| macOS 并发测试超过固定毫秒上限。 | 共享 runner 调度延迟使耗时断言误报。 | 正确性用 channel 等同步信号验证重叠和依赖顺序；宽松超时仅防挂起。吞吐、延迟等性能指标放到受控性能评估中。 |
| macOS job 无法取得 runner，或两个架构配置不一致。 | 使用退役镜像，或 CI / Release matrix 漂移。 | CI 与 Release 同步维护 runner label、架构、Rust target 和 artifact suffix；当前使用 macOS 15 ARM64 / Intel。升级前核实 GitHub runner 支持状态。 |
| Release workflow 表达式无效，或发布阶段权限不足。 | 在 Actions 表达式中混用 Bash 参数展开，或发布 job 没有写权限。 | 版本处理在 Bash 中执行 `${GITHUB_REF_NAME#v}`，再通过 step output 传递；仅发布 job 使用 `contents: write`。workflow 修改运行 actionlint，版本提取覆盖 stable / prerelease tag。 |
| Windows 路径 / TOML fixture / 文本匹配失败，或 Java 中文内容异常。 | 假定 Unix 绝对路径、直接把反斜杠路径嵌入 TOML、忽略 CRLF，或使用默认源码编码。 | 路径断言使用 `Path`；绝对路径用平台实际路径；TOML 路径走统一 helper；多行匹配先归一化换行；Java 显式 `javac -encoding UTF-8`。 |
| Linux 严格 Clippy 报平台专用 import 未使用。 | import 的编译条件与使用它的测试不一致。 | 平台专用 import 和使用处共享 `cfg` 边界；Windows 本地 lint 不能代替 Linux lint，避免用全局 `allow(unused_imports)` 掩盖问题。 |
| 根 `cargo test` 通过，但独立 runtime 包仍有失败。 | `runtime/rust` 的四个 crate 不属于根 workspace；可选工具缺失也会使部分语言 harness 跳过。 | 逐包运行 fmt、严格 Clippy 和测试；检查 shared compliance corpus 新增 case 是否在各 runner 中执行。需要的语言工具显式设为 required，并审阅 executed/skipped/failed summary。 |

## npm 测试入口的复现方法

在临时干净检出中的 `runtime/typescript` 目录，使用与 CI 一致的 Node 24：

```sh
node --version
npm --version
npm ci --include=dev
npm run typecheck
npm test
```

`npm test` 先按依赖顺序构建五个 package，再由 `node --test` 发现测试。
当前应执行 3 个真实 MessagePack backend 测试，覆盖成功解码、错误归一化 / binary 拒绝与大小限制。
新增测试后应同步更新此数量；缺少依赖或构建产物应修复安装 / 构建流程，不能通过跳过测试绕开。
跨平台脚本不要依赖 shell 展开通配符；没有必要为测试目录手写文件枚举器。

已确认的本次远端证据见
[run 41167835837](https://github.com/watsonsong/ConfigForge/actions/runs/41167835837)：
commit `1ec57de` 的 quality job 在 Node `24.22.1` 下完成 typecheck / build 后，
因 `node --test tests` 报 `MODULE_NOT_FOUND` 退出。编译器升级与依赖安装均已成功，
此次修复只调整测试发现入口，不改变 runtime API 或解码逻辑。

其他规则的修复依据可查提交 `0f44eab`（runner / Release）、`e0f4f33`（平台 import）、
`f0e0817`（跨平台 fixture）、`81fa682`（.NET runtime）、`234d210`（TypeScript helper）、
`ddce24d`（并发测试）和 `e1a1234`（独立 Rust runtime 检查）。
