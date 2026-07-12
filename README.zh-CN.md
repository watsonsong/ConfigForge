[English](README.md) | [简体中文](README.zh-CN.md)
[![CI Status](https://github.com/watsonsong/ConfigForge/actions/workflows/ci.yml/badge.svg)](https://github.com/watsonsong/ConfigForge/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/config-forge.svg)](https://crates.io/crates/config-forge)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![文档](https://img.shields.io/badge/文档-最新-green.svg)](Documentation/README.md)
# ConfigForge
ConfigForge 是一款**AI驱动、Schema优先的游戏配置导出工具**，使用Rust构建，为游戏项目提供端到端配置工作流：声明式定义表结构、全量校验策划配置数据、开箱即用生成7+语言/平台的类型安全运行时产物。
项目专为AI辅助开发设计，你可以通过提供的架构上下文文档配合AI工具，在几分钟内完成自定义验证器、导出器、代码生成器或运行时适配器的开发。
## ✨ 核心特性
- 📋 **Schema驱动工作流**：使用声明式TOML元文件定义表、字段、类型、索引、外键和约束，作为唯一可信源
- ✅ **确定性校验**：导出前全量检测数据错误（类型不匹配、引用缺失、非法值、资源路径错误），永远不会将错误配置发布到生产环境
- 📦 **多格式数据导出**：开箱支持JSON、MessagePack、优化二进制、Lua内嵌表、CSV、i18n POT文件、校验报告，可轻松扩展自定义格式
- 🔌 **多语言代码生成**：生成类型安全、IDE友好的数据访问代码，支持Rust、C++（含Unreal Engine）、C#、Java、TypeScript、Lua，更多语言持续新增中
- 🛡️ **内置验证器**：提供游戏开发常用验证规则：正则、范围、字符串长度、外键、资源引用、字段求和、单调序列、条件必填等
- 🎮 **游戏管线友好**：原生适配Unreal/Unity项目结构，支持资源引用校验/路径补全、资源清单生成、目标平台配置过滤（客户端/服务端/地区专属构建）
- 🤖 **AI优先可扩展性**：清晰的模块化架构边界，将提供的AI上下文文档投喂给你常用的AI助手，无需通读全量代码即可开发自定义扩展
## 🚀 5分钟快速上手
### 1. 安装
```bash
# 通过cargo安装
cargo install config-forge
# 或从[Releases页面](https://github.com/watsonsong/ConfigForge/releases)下载预编译二进制
```
### 2. 运行完整示例
```bash
git clone https://github.com/watsonsong/ConfigForge.git
cd ConfigForge/examples/full_game_demo
config-forge export
```
### 3. 查看输出
生成的产物将保存在`out/`目录下，包括：
- 运行时数据文件（JSON/MessagePack/二进制/Lua）
- 所有支持语言的类型安全访问代码
- 校验报告（HTML/Markdown/JSON/CSV格式）
- 自动生成的Schema文档
- 资源清单和i18n POT翻译模板
## 📚 文档
- [用户手册&FAQ](Documentation/Design/07-user-manual-and-qa.md) - 面向策划和开发者的完整使用指南
- [架构文档索引](Documentation/README.md) - 完整设计、实现和技术文档
- [AI扩展开发指南](Documentation/Design/ai-extension-guide.md) - 使用AI辅助开发自定义扩展（即将上线）
- CLI参考：运行`config-forge --help`查看完整命令文档
## 🤝 参与贡献
欢迎提交贡献！请参考[CONTRIBUTING.md](CONTRIBUTING.md)（即将上线）了解开发环境搭建、贡献指南，以及如何使用AI工具高效参与开发。
- 通过[GitHub Issues](https://github.com/watsonsong/ConfigForge/issues)提交Bug报告或功能需求
- 提交PR请遵循[Documentation/code-style.md](Documentation/code-style.md)中定义的代码规范和测试要求
## 📄 开源协议
ConfigForge 基于 Apache License 2.0 协议开源，完整协议内容请查看[LICENSE](LICENSE)文件。