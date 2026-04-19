# SPG 五代理治理技能说明

本仓库已收录 `five-agent-governance` 技能源码，路径如下：

- `skills/five-agent-governance/`

该技能用于在项目线程开始时，以真实五子代理模式启动治理链路：

- `Maxwell`
- `Dirac`
- `Parfit`
- `Wegener`
- `Popper`

核心规则：

- 项目线程以固定启动协议开场
- 五个真实子代理全部成功启动后，才进入正式治理
- `Wegener` 必须完成两轮质询与两轮自审
- `Popper` 必须完成三轮反问与最终五态裁决
- 未完成启动或裁决时，不允许伪装成完整治理

## 目录

- `SKILL.md`
- `agents/openai.yaml`
- `references/`
  - `protocol-overview.md`
  - `startup-protocol.md`
  - `maxwell-discovery.md`
  - `wegener-interrogation.md`
  - `popper-verdict.md`
  - `thread-checkpoints.md`
  - `output-contracts.md`
  - `checklists.md`
  - `samples.md`

## 安装

可直接执行：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-five-agent-governance-skill.ps1
```

默认安装到：

```text
%USERPROFILE%\.codex\skills\five-agent-governance
```

## 启动示例

```text
本线程启用 five-agent-governance 真实五代理模式。真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper。当前阶段是新项目启动。
```
