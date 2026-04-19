# Samples

Use these samples when opening or governing a real five-agent project thread.

## Sample startup line

```text
Enable five-agent-governance real five-subagent mode in this project thread.
Launch Maxwell, Dirac, Parfit, Wegener, and Popper as real subagents.
Current stage: new project startup.
Current checkpoint: requirements-freeze.
Current goal: stabilize the problem definition before solutioning.
```

## Sample Chinese startup line

```text
本线程启用 five-agent-governance 真实五代理模式。
真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper。
当前阶段：新项目启动。
当前检查点：需求冻结前。
当前目标：先把问题定义稳定下来，再进入方案阶段。
```

## Sample boot report

```text
ThreadBootStatus
- thread_type: project
- startup_protocol_detected: true
- agents_requested: Maxwell, Dirac, Parfit, Wegener, Popper
- agents_started: Maxwell, Dirac, Parfit, Wegener, Popper
- agents_failed: none
- boot_status: complete
- can_enter_governance: true
```

## Sample no-go boot report

```text
ThreadBootStatus
- thread_type: project
- startup_protocol_detected: true
- agents_requested: Maxwell, Dirac, Parfit, Wegener, Popper
- agents_started: Maxwell, Dirac, Parfit
- agents_failed: Wegener, Popper
- boot_status: incomplete
- can_enter_governance: false
```

## Sample final verdict line

```text
GovernanceVerdict
- verdict: WAIT
- reason: key assumptions are still unconfirmed
- required_actions: confirm the unresolved operating constraints
- retry_scope: restart from Maxwell clarification
- next_allowed_stage: requirements-freeze
```
