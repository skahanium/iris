-- 清理 files 表中同 path 的重复行（保留 id 最大的一条）
DELETE FROM files
WHERE id NOT IN (SELECT MAX(id) FROM files GROUP BY path);

CREATE UNIQUE INDEX IF NOT EXISTS idx_files_path_unique ON files(path);
