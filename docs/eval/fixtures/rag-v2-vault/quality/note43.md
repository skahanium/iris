---
title: "持续集成与持续交付CI/CD流水线"
aliases: ["CI/CD", "持续集成", "持续交付", "pipeline"]
tags: ["area-quality", "fixture", "软件质量", "CICD", "DevOps"]
---

# 持续集成与持续交付CI/CD流水线

持续集成（Continuous Integration, CI）和持续交付（Continuous Delivery, CD）是现代软件工程中不可或缺的自动化实践。CI 要求开发团队成员频繁地将代码变更合并到主干分支，每次合并触发自动化的构建和测试流程。CD 在 CI 的基础上进一步自动化部署流程，使通过验证的代码变更可以随时安全地发布到生产环境。

CI 流水线的标准阶段通常包括：代码检出、依赖安装、静态代码分析（Lint）、编译构建、单元测试、集成测试和安全扫描。静态代码分析工具如 SonarQube 和 ESLint 在代码提交后秒级反馈代码异味和安全漏洞。流水线的"快速失败"原则要求将最快执行的检查放在最前——构建失败应在秒级反馈，单元测试应在分钟内完成，集成测试可以在更长的时间窗口内运行。

证据令牌: evaltok43

CI/CD 的基础设施已从 Jenkins 式的自托管模式向云原生平台演进。GitHub Actions 通过 YAML 定义的 Workflow 与代码库版本存储在同一仓库中，实现了"流水线即代码（Pipeline as Code）"的理念——CI 配置和代码一起进行版本控制、代码审查和变更追溯。GitLab CI 和 CircleCI 提供了类似的容器化 Runner 架构。Dagger 等新兴工具进一步将 CI/CD 逻辑抽象为可编程的 DAG（有向无环图），使流水线可以在本地和 CI 环境中一致执行。

持续交付与持续部署的区别在于最后一步的人工决策权。持续交付（Continuous Delivery）要求系统在任何时刻都处于可部署状态，但生产部署的触发按钮由人工按下。持续部署（Continuous Deployment）则在每次通过全量测试后自动部署到生产环境，这一模式适用于拥有强大监控和自动回滚能力的成熟组织，如 Netflix 和 Amazon 每天进行数千次生产部署。无论采用何种模式，蓝绿部署（Blue-Green Deployment）和金丝雀发布（Canary Release）都是降低部署风险的标准策略。
