---
title: "模型性能监控与告警体系"
aliases: ["模型监控", "ML-monitoring", "模型性能"]
tags: ["area-ai", "fixture", "MLOps", "监控", "模型运维"]
---

# 模型性能监控与告警体系

机器学习模型在生产环境中的性能并非一成不变。数据分布的漂移、业务逻辑的变更、外部环境的变化都可能导致模型预测质量逐步下降，这种现象被称为模型衰减（Model Decay）。建立完善的模型监控体系是 MLOps 的核心能力之一，Google 在《Machine Learning: The High-Interest Credit Card of Technical Debt》论文中指出，缺乏监控的机器学习系统承担着隐性的高额技术债务。

模型监控的指标体系通常分为四类。预测质量指标包括准确率、精确率、召回率、AUC、F1 等，但生产环境中往往缺乏即时标注的真值（Ground Truth），需要通过延迟反馈或人工抽检来估算。数据质量指标监控特征缺失率、特征值范围异常、类别分布变化等数据层面的健康信号。系统性能指标关注推理延迟（P50/P95/P99）、吞吐量和资源利用率。业务指标则跟踪模型对实际业务 KPI 的影响。

证据令牌: evaltok30

漂移检测（Drift Detection）是模型监控中最关键的环节。数据漂移（Data Drift）指输入特征的统计分布随时间发生变化；概念漂移（Concept Drift）指特征到标签的映射关系发生变化。KS 检验、卡方检验、Wasserstein 距离和 MMD（Maximum Mean Discrepancy）是常用的统计漂移检测方法。Evidently AI、Great Expectations 和 WhyLogs 等开源工具提供了开箱即用的漂移检测和监控仪表盘。

告警策略需要平衡灵敏度与噪声。过低的告警阈值导致告警疲劳（Alert Fatigue），运维人员会逐渐忽视所有告警；过高的阈值则可能遗漏真正的性能退化。实践中通常采用多级告警策略：轻微漂移生成非紧急的可视化报告，中等漂移触发团队的 Slack 通知或工单，严重漂移则自动触发模型回滚或切换至备用模型。
