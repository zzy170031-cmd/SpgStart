# SPG Governance Index

This file is the stable governance entry point for the repository.

Use it when you need to understand:

- what the current governance baseline is
- how to start a new real five-agent project thread
- which documents are active
- which documents are legacy archive only

## Current Baseline

The current project-thread baseline is the real `five-agent-governance` skill:

- [five-agent-governance skill](../skills/five-agent-governance/SKILL.md)
- [Five-Agent Skill Guide](./SPG_五代理治理技能说明.md)
- [Five Agent Thread Startup Template](./five-agent-thread-startup-template.md)

## Active Governance Chain

- `Maxwell`
  Need clarification, scope, priority, success criteria
- `Dirac`
  Technical shape, implementation path, module boundaries
- `Parfit`
  UX, testability, ops, consistency, maintainability
- `Wegener`
  Two interrogation rounds, two self-reviews, integrated candidate plan
- `Popper`
  Three challenge rounds and final verdict

## Current Working Docs

These are active documents and should be treated as current:

- [README](../README.md)
- [Five-Agent Skill Guide](./SPG_五代理治理技能说明.md)
- [Five Agent Thread Startup Template](./five-agent-thread-startup-template.md)
- [Process Overview](./SPG_全流程说明.md)
- [Implementation Logic](./SPG产品实现逻辑.md)
- [Workbench Sidecar Boundary](./SPG_Workbench_Sidecar边界说明.md)

## Legacy Archive

These are historical records and should not be used as the default rule for new project threads:

- [Four-Agent Collaboration Archive](./SPG_四代理协作机制.md)
- [Third-Page Historical Snapshot (2026-04-14)](./third-page-next-step-2026-04-14.md)

## How To Start A New Project Thread

Use the startup template directly, or paste one of the following minimal versions:

### English

```text
Enable five-agent-governance real five-subagent mode in this project thread. Launch Maxwell, Dirac, Parfit, Wegener, and Popper as real subagents. Current stage: new project startup.
```

### Chinese

```text
本线程启用 five-agent-governance 真实五代理模式。真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper。当前阶段是新项目启动。
```

## Install / Reinstall The Skill

From the repo root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-five-agent-governance-skill.ps1
```

Default install target:

```text
%USERPROFILE%\.codex\skills\five-agent-governance
```
