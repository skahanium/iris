/**
 * v0.1 编辑器「核心 GFM」范围说明。
 *
 * 数据流：`.md` → `markdownToHtml` (marked + gfm) → TipTap；
 * 保存：TipTap HTML → `htmlToMarkdown` (turndown + turndown-plugin-gfm)。
 *
 * 本文件供 UI/文档引用；行为回归见 `tests/markdown_roundtrip.test.ts`。
 */

/** TipTap schema 与序列化链路已覆盖、测试保障的 GFM 子集 */
export const SUPPORTED_CORE_GFM = [
  "标题（ATX # … ######）",
  "段落与换行",
  "粗体、斜体、删除线",
  "行内代码与围栏代码块（含语言标识）",
  "无序/有序列表",
  "任务列表（- [ ] / - [x]）",
  "GFM 表格",
  "引用块（>）",
  "链接 [text](url)",
] as const;

/**
 * v0.1 未纳入编辑器 schema 或未保证往返的语法。
 * marked 可能解析为 HTML，但 TipTap 会剥离或 turndown 无法还原为等价 Markdown。
 */
export const UNSUPPORTED_OR_BEST_EFFORT_GFM = [
  "脚注 ([^1])",
  "数学公式 ($…$ / $$…$$)",
  "定义列表",
  "自动链接尖括号 <url>",
  "图片 ![alt](url)（无 image 节点）",
  "HTML 内嵌标签",
  "嵌套任务列表与复杂表格合并单元格",
  "目录 TOC、emoji 短代码（视解析器而定）",
] as const;
