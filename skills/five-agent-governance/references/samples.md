# Samples

## Sample startup line

"本线程启用 five-agent-governance 真实五代理模式。真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper。当前阶段是新项目启动，目标是先完成需求冻结前的治理。"

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
