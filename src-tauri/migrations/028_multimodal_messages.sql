-- 为 session_messages 新增多模态内容列
-- content 保留为文本摘要/占位，content_parts 存储完整的 ContentPart[] JSON
ALTER TABLE session_messages ADD COLUMN content_parts TEXT;

-- content_parts 为 NULL 时消息视为纯文本（向后兼容）
-- content_parts 有值时消息内容以 content_parts 为准
