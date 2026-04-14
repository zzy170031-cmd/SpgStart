# SPG

SPG 是一个以 Rust 为核心的桌面化 SLG 热点工作台，当前主线目标是围绕抖音上的 SLG 内容，完成从 API 配置、真实数据发现、内容拆解、热度聚合到创作指导的闭环。

当前主线特征：

- Windows 桌面启动器
- 三段闸门流程：本地用户确认 -> API 配置测试 -> 主系统工作台
- Rust workspace：`spg-core` / `spg-web` / `spg-desktop`
- 主系统工作台：游戏搜索、热度可视化、抖音视频拆解、创作指导、API 调用实况
- 固定四代理协作机制，作为工程主线的一部分长期复用

## Workspace

- `crates/spg-core`
  负责实体识别、规则处理、SQLite 持久化和核心输入链路
- `crates/spg-web`
  负责本地 Web 服务、页面路由、Provider 配置、Workbench 调度和主系统 API
- `crates/spg-desktop`
  负责 Windows 桌面壳、窗口、托盘和本地服务引导

## Current Product Scope

- 平台：`douyin`
- 首批监控游戏：
  - 率土之滨
  - 三国群英传策定九州
  - 三国谋定天下
  - 无尽冬日
  - 三国冰河时代
  - 天下归心
  - 九牧之野
- 当前主分析模型：`doubao-seed-2-0-code-preview-260215`

## Core Docs

- [SPG 全流程说明](./docs/SPG_全流程说明.md)
- [SPG 产品实现逻辑](./docs/SPG产品实现逻辑.md)
- [SPG 四代理协作机制](./docs/SPG_四代理协作机制.md)

## Engineering Rules

- 主实现语言固定为 Rust；前端 JS/CSS 仅承担展示与交互层职责
- 新仓库只承接当前确认的生产主线，不引入历史废弃页面和临时实验资产
- 每次主线更新后，按固定四代理机制复盘：
  - Maxwell：产品经理
  - Dirac：软件工程师
  - Parfit：产品 UI 与产品测试运维
  - Wegener：总设计师

## Local Verification

常用验证命令：

```powershell
cargo test --workspace
cargo build -p spg-desktop
```
