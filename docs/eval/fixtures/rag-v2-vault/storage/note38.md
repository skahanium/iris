---
title: "LSM-Tree压缩合并策略的权衡分析"
aliases: ["LSM压缩", "compaction-strategies", "LSM合并策略"]
tags: ["area-storage", "fixture", "存储引擎", "LSM-tree", "压缩合并"]
---

# LSM-Tree压缩合并策略的权衡分析

压缩合并（Compaction）是 LSM-Tree 存储引擎中决定长期读写性能和维护成本的核心过程。随着写入持续进行，LSM-Tree 在多个层级中不断累积 SSTable 文件，若不及时合并清理，读放大（Read Amplification）将变得无法忍受——一次点查询可能需要对数十甚至数百个 SSTable 进行布隆过滤器检测。Compaction 通过选取一定范围内的 SSTable 文件进行归并排序，删除过期的版本和已标记删除的墓碑记录，从而将数据集中压缩到更少的文件中。

RocksDB 提供了三种主要的 Compaction 策略。Leveled Compaction 是默认策略，将每一层（除了 Level 0）的大小维持为上一层的固定倍数（通常 10 倍），当某层的文件总大小超过阈值时触发合并。Leveled Compaction 优化了空间放大（Space Amplification），磁盘总使用量约为活跃数据量的 1.11 倍，但写放大（Write Amplification）较高，因为每个键值对在层间移动时会被反复重写。

证据令牌: evaltok38

Tiered Compaction（又称 Universal Compaction）以空间放大换取更低的写放大。该策略将各层级的大小设置为层级序号的幂次增长，每次 Compaction 将整个层的所有文件和相邻层合并，写入次数显著少于 Leveled 策略。这使其非常适合写入密集且磁盘空间充裕的场景，如 Kafka 的消息持久化层。

FIFO Compaction 是最轻量的策略，当 SSTable 文件总大小超过阈值时直接删除最旧的文件，不执行任何合并操作。它适用于写后永不修改且 TTL 自动过期的时间序列数据。选择 Compaction 策略本质上是在写放大、读放大和空间放大三个维度间的三角权衡，没有全局最优解。RocksDB 的 Dynamic Compaction 特性允许在运行时根据工作负载特征自动调整合并参数。
