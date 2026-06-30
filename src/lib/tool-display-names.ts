/** 工具名 → 底栏/日志用中文展示名 */
export const TOOL_DISPLAY_NAMES: Record<string, string> = {
  search_hybrid: "混合搜索",
  search_semantic: "语义搜索",
  search_keyword: "关键词搜索",
  get_regulation: "法规查询",
  get_context_packets: "获取证据包",
  get_genre_template: "获取文种模板",
  get_model_essays: "获取范文",
  get_block_links: "获取块级链接",
  read_note: "读取笔记",
  list_vault: "列出笔记库",
  get_outline: "文档大纲",
  get_backlinks: "反向链接",
  web_search: "联网搜索",
  insert_text_at_cursor: "插入文本",
  replace_selection: "替换选区",
  add_tags: "添加标签",
  spawn_subagent: "子任务",
  conclude_reasoning: "推理收尾",
};

export function toolDisplayName(name: string): string {
  return TOOL_DISPLAY_NAMES[name] ?? name;
}
