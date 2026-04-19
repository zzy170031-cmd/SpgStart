# Output Contracts

Use these contracts to keep cross-thread governance stable.

## ThreadBootStatus

- `thread_type`
- `startup_protocol_detected`
- `agents_requested`
- `agents_started`
- `agents_failed`
- `boot_status`
- `can_enter_governance`

## ThreadGovernanceState

- `stage`
- `checkpoint`
- `mode`
- `last_verdict`
- `discovery_enabled`

## AgentPosition

- `agent_name`
- `role`
- `position`
- `risks`
- `recommended_action`

## WegenerInterrogationRecord

- `round`
- `target_agent`
- `challenge_points`
- `assessment`

## WegenerSelfReviewRecord

- `round`
- `focus`
- `findings`

## PopperChallengeRecord

- `round`
- `focus`
- `assessment`

## GovernanceVerdict

- `verdict`
- `reason`
- `required_actions`
- `retry_scope`
- `next_allowed_stage`
