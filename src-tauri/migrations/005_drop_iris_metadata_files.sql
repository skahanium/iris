-- 移除误索引的 .iris 元数据（版本快照等），仅保留用户 vault 中的笔记
DELETE FROM files WHERE path LIKE '.iris/%';
