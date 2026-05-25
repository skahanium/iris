---
title: 文件监听
tags: [存储]
---

# vault 文件监听

notify 递归监听笔记目录，外部修改 `.md` 后触发 file:changed 事件并更新索引。切换 vault 时重建 FileWatcher。
