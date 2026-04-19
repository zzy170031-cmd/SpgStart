# SPG Workbench Sidecar 边界说明

这份文档用于说明当前 SPG 主工程里，`Workbench / AutoReason / Hermes` 一类 sidecar 增强能力的正式边界，避免后续开发把实验性 sidecar 层误当成核心主链路。

> Governance note: sidecar 相关改动仍然要服从当前 `five-agent-governance` 基线。旧四代理文档不再是默认治理入口。

## 结论

当前主线接受 `spg-web / workbench` 内部的 sidecar 能力，但它只能作为工作台边缘增强层存在，不能越界改写 `spg-core` 的正式语义。

## 允许进入主线的范围

- `crates/spg-web/src/workbench.rs`
- `crates/spg-web/src/models.rs`
- 与 workbench 页面展示直接相关的前端资产
- 与 workbench 运行结果直接相关的说明文档

这些内容可以继续演进，但必须服从当前主系统主线目标：

- Douyin SLG 热点发现
- 内容拆解
- 创作指导
- 选题池沉淀
- API 预算感知与调度

## 明确禁止越界的范围

以下内容不允许被 sidecar 逻辑直接改写语义：

- `crates/spg-core`
- `pipeline.process`
- 正式 ranking 语义
- 主实体识别规则的最终口径
- 三段门禁的主流程规则

如果 sidecar 需要影响这些区域，必须经过主线重构决策，而不能以实验性增强名义直接侵入。

## 当前主线态度

- `AutoReason` 默认作为关闭或 shadow 可观测能力
- 它可以记录额外判断、辅助 refinement、补充 workbench 观察数据
- 它不能直接成为新的唯一真值来源
- 当 sidecar 结果与主链路冲突时，以主链路稳定运行和可解释输出为先

## 迁移到新仓库后的执行原则

sidecar 相关改动继续允许存在，但必须遵守下面四条：

1. 只在 `spg-web/workbench` 边界内活动
2. 任何越界到 `spg-core` 的语义改动，都必须单独立项
3. 不能因为 sidecar 实验破坏桌面启动页、Provider 配置页和主工作台主链路
4. 每次更新后，仍然要经过当前五代理治理机制复盘，再决定是否保留
