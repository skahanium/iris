---
title: "预写日志WAL与崩溃恢复机制"
aliases: ["WAL", "预写日志", "write-ahead-log", "崩溃恢复"]
tags: ["area-storage", "fixture", "存储引擎", "WAL", "持久性"]
---

# 预写日志WAL与崩溃恢复机制

预写日志（Write-Ahead Logging, WAL）是数据库系统中保障事务持久性（Durability）的核心机制，体现了 ACID 中 D 属性的工程技术实现。WAL 的核心规则简单而严格：在对数据页进行任何修改之前，必须先将描述修改内容的日志记录持久化到磁盘上的日志文件中。这一"先写日志再写数据"的顺序保证了系统在任意时刻崩溃后都能通过重放日志恢复到一致状态。

WAL 日志记录包含事务开始的标识（如 LSN，Log Sequence Number）、被修改页面的标识符、修改前的旧值（Undo）和修改后的新值（Redo）。LSN 是单调递增的全局序列号，每个数据页的头部也保存着最后修改该页的日志记录的 LSN。崩溃恢复时，恢复管理器从最后一个检查点（Checkpoint）开始扫描日志，对所有已提交事务的 Redo 记录进行前滚（Roll Forward），对所有未提交事务的 Undo 记录进行回滚（Roll Back）。

证据令牌: evaltok40

WAL 的磁盘写入模式直接影响性能。默认的 fsync 每次提交都将日志刷写到磁盘，提供最强持久性保证但吞吐量最低。组提交（Group Commit）将多个并发事务的日志记录批量刷写，是 OLTP 系统的关键性能优化——在高并发场景下可将写入吞吐量提升一个数量级。MySQL InnoDB 的 innodb_flush_log_at_trx_commit 参数和 PostgreSQL 的 synchronous_commit 参数都允许用户在不同持久性级别和写入性能之间做出权衡。

检查点（Checkpoint）机制是 WAL 回收磁盘空间和控制恢复时间的关键。检查点将所有内存中的脏页刷入磁盘后，该检查点 LSN 之前的所有 WAL 日志即可安全回收。频繁的检查点减少恢复时间但会增加写入负载；稀疏的检查点降低运行时开销但延长崩溃恢复时间。现代数据库通常采用模糊检查点（Fuzzy Checkpoint），在后台逐步刷写脏页而不阻塞并发事务，在恢复效率和运行时性能之间取得良好平衡。
