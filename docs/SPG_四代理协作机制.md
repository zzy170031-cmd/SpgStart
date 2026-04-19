# SPG 四代理协作机制（历史归档）

> Superseded: this document records the older four-agent collaboration model and is kept only for historical traceability. New project threads should use the real `five-agent-governance` skill and its startup protocol instead.

## 当前状态

这份文档不再是 SPG 的现行治理规则。现行入口请看：

- `skills/five-agent-governance/`
- [SPG_五代理治理技能说明.md](./SPG_五代理治理技能说明.md)
- [five-agent-thread-startup-template.md](./five-agent-thread-startup-template.md)

## 历史上的四代理定义

旧机制曾固定使用以下四个角色：

- `Maxwell`：产品经理
- `Dirac`：软件工程师
- `Parfit`：产品 UI 与产品测试运维
- `Wegener`：总设计师

它的核心工作方式是：

1. `Maxwell / Dirac / Parfit` 先各自独立检查。
2. `Wegener` 汇总三方意见。
3. `Wegener` 对三方做两轮质询。
4. `Wegener` 输出最后的综合结论。

## 为什么它被替代

旧四代理机制保留了“多视角审查”的优点，但它缺少一个独立的最终门禁角色，导致：

- `Wegener` 同时承担整合与放行，裁决权过重
- 历史线程里容易把“整合方案”和“最终放行”混成一步
- 缺少明确的 `WAIT / NO / ESCALATE` 门禁语义

因此现行机制升级为五代理：

- `Maxwell`
- `Dirac`
- `Parfit`
- `Wegener`
- `Popper`

新增的 `Popper` 专门负责最终反问与裁决，避免把旧四代理直接换皮成新名字。

## 与现行五代理的关系

如果你在阅读旧记录时看到了四代理结论，可以这样理解：

- `Maxwell / Dirac / Parfit` 的职责思路仍有参考价值
- `Wegener` 的整合与质询逻辑被保留
- 最终裁决权已经迁移到 `Popper`
- 新线程不再以这份文档作为默认工作协议

## 迁移规则

- 保留本文件，目的是保留演进痕迹，不是继续作为现行规范
- 后续 README、流程文档、实现逻辑文档，一律以五代理治理 skill 为准
- 若需要启动新项目线程，请直接使用五代理启动模板，而不是引用本文件
