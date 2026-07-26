/**
 * CJK 标点自动转换纯逻辑。
 *
 * 在中文上下文里把 ASCII 标点转成全角对应字符；智能引号按当前文本块内
 * 未配对的 `“` / `‘` 计数决定开/闭。仅当紧邻前一个字符属于 CJK 上下文
 * （Han / Hiragana / Katakana / Hangul 或全角符号区）时才转换，从而保护
 * `1.` 有序列表、`www.example.com`、英文段落与 markdown 触发符。
 */

/** ASCII 标点 → 全角映射（非引号）。 */
export const CJK_PUNCT_MAP: Record<string, string> = {
  ".": "。",
  ",": "，",
  ":": "：",
  ";": "；",
  "!": "！",
  "?": "？",
  "(": "（",
  ")": "）",
};

/** 智能双引号对。 */
export const CJK_DOUBLE_QUOTES = { open: "“", close: "”" } as const;
/** 智能单引号对。 */
export const CJK_SINGLE_QUOTES = { open: "‘", close: "’" } as const;

const CJK_CONTEXT_RE =
  /^[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\u2018\u2019\u201C\u201D\u3000-\u303F\uFF00-\uFFEF]$/u;

/**
 * 判断一个字符是否属于 CJK 上下文（决定是否触发转换）。
 * 空字符串（行首/块首）返回 false。
 */
export function isCjkContextChar(ch: string): boolean {
  if (ch === "") return false;
  return CJK_CONTEXT_RE.test(ch);
}

export interface CjkPunctConversion {
  /** 转换后的字符（未命中时等于输入）。 */
  converted: string;
  /** 是否发生了转换。 */
  changed: boolean;
}

/**
 * 把单个 ASCII 标点字符在 CJK 上下文中转换为全角对应字符。
 *
 * @param input 待转换的单字符
 * @param beforeChar 紧邻前一个字符（用于判断 CJK 上下文）
 * @param unmatchedOpenDouble 当前文本块内未配对的 `“` 数量
 * @param unmatchedOpenSingle 当前文本块内未配对的 `‘` 数量
 */
export function convertAsciiPunctChar(
  input: string,
  beforeChar: string,
  unmatchedOpenDouble: number,
  unmatchedOpenSingle: number,
): CjkPunctConversion {
  if (input.length !== 1) {
    return { converted: input, changed: false };
  }

  // 引号需要 CJK 上下文 + 配对计数
  if (input === '"') {
    if (!isCjkContextChar(beforeChar)) {
      return { converted: input, changed: false };
    }
    const close = unmatchedOpenDouble > 0;
    return {
      converted: close ? CJK_DOUBLE_QUOTES.close : CJK_DOUBLE_QUOTES.open,
      changed: true,
    };
  }

  if (input === "'") {
    if (!isCjkContextChar(beforeChar)) {
      return { converted: input, changed: false };
    }
    const close = unmatchedOpenSingle > 0;
    return {
      converted: close ? CJK_SINGLE_QUOTES.close : CJK_SINGLE_QUOTES.open,
      changed: true,
    };
  }

  const mapped = CJK_PUNCT_MAP[input];
  if (mapped === undefined) {
    return { converted: input, changed: false };
  }
  if (!isCjkContextChar(beforeChar)) {
    return { converted: input, changed: false };
  }
  return { converted: mapped, changed: true };
}

/**
 * 统计文本块内未配对的智能引号数量（用于开/闭决策）。
 * 仅统计全角 `“”` 与 `‘’`，忽略 ASCII 引号。
 */
export function countUnmatchedSmartQuotes(text: string): {
  double: number;
  single: number;
} {
  let double = 0;
  let single = 0;
  for (const ch of text) {
    if (ch === CJK_DOUBLE_QUOTES.open) {
      double += 1;
    } else if (ch === CJK_DOUBLE_QUOTES.close) {
      if (double > 0) double -= 1;
    } else if (ch === CJK_SINGLE_QUOTES.open) {
      single += 1;
    } else if (ch === CJK_SINGLE_QUOTES.close) {
      if (single > 0) single -= 1;
    }
  }
  return { double, single };
}
