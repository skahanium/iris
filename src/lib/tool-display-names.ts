/** 工具名 → 底栏/日志用中文展示名 */
export const TOOL_DISPLAY_NAMES: Record<string, string> = {
  search_hybrid: "混合搜索",
  search_semantic: "语义搜索",
  search_keyword: "关键词搜索",
  get_regulation: "法规查询",
  get_context_packets: "获取证据包",
  get_genre_template: "获取文种模板",
  get_model_essays: "获取范文",
  read_note: "读取笔记",
  list_vault: "列出笔记库",
  get_outline: "文档大纲",
  get_backlinks: "反向链接",
  web_search: "联网搜索",
  web_fetch: "读取网页",
  system_time_now: "读取本机时间",
  app_context_read: "读取应用上下文",
  capabilities_read: "读取能力摘要",
  insert_text_at_cursor: "插入文本",
  replace_selection: "替换选区",
  add_tags: "添加标签",
  git_read_diff: "读取代码差异",
  git_read_log: "读取提交记录",
  git_read_status: "读取仓库状态",
  memory_read: "读取记忆",
  memory_write: "更新记忆",
  scheduled_task_create: "创建定时任务",
  scheduled_task_list: "列出定时任务",
  scheduled_task_delete: "删除定时任务",
  secret_exists: "检查密钥配置",
  skills_list: "列出技能",
  vault_version_list: "列出笔记版本",
  vault_create_note: "创建笔记",
  vault_rename_move: "移动笔记",
  vault_delete_to_trash: "移至废纸篓",
  vault_asset_write: "写入附件",
  spawn_subagent: "子任务",
  conclude_reasoning: "推理收尾",
};

export function toolDisplayName(name: string): string {
  return (
    TOOL_DISPLAY_NAMES[name] ??
    TOOL_DISPLAY_NAMES[name.replaceAll(".", "_")] ??
    "执行工具"
  );
}
