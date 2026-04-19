# Five Agent Thread Startup Template

Use this template whenever you open a new project thread that should start in real `five-agent-governance` mode.

## Minimal startup template

```text
本线程启用 five-agent-governance 真实五代理模式。
真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper。
当前阶段：新项目启动。
```

## Standard startup template

```text
本线程启用 five-agent-governance 真实五代理模式。
真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper。
当前阶段：新项目启动。
当前检查点：需求冻结前。
当前目标：先把问题定义、边界和成功标准收紧，再进入方案治理。
当前模式：Full Council。
```

## Feature implementation template

```text
本线程启用 five-agent-governance 真实五代理模式。
真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper。
当前阶段：功能实现推进。
当前检查点：实现前。
当前目标：在已有需求基础上，完成实现路径、风险检查和放行判断。
当前模式：Fast Gate。
```

## Merge gate template

```text
本线程启用 five-agent-governance 真实五代理模式。
真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper。
当前阶段：合并前检查。
当前检查点：pre-merge。
当前目标：确认当前变更是否满足合并条件。
当前模式：Full Council。
```

## Release gate template

```text
本线程启用 five-agent-governance 真实五代理模式。
真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper。
当前阶段：发布前检查。
当前检查点：pre-release。
当前目标：确认当前版本是否满足发布条件。
当前模式：Full Council。
```

## Recommended usage notes

- Use `Full Council` for new projects, major features, merge gates, and release gates.
- Use `Fast Gate` only when the project thread has already passed earlier checkpoints.
- If the five agents do not all start successfully, do not continue as if full governance is active.
- Keep the startup message at the top of the thread so later review can clearly see when governance began.
