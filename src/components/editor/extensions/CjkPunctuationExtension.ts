import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

import {
  CJK_PUNCT_MAP,
  convertAsciiPunctChar,
  countUnmatchedSmartQuotes,
  isCjkContextChar,
} from "@/lib/cjk-punctuation";

/** 运行时判定开关：调用方传闭包读取稳定 ref，切换时无需重建编辑器。 */
export interface CjkPunctuationOptions {
  isEnabled: () => boolean;
}

export const cjkPunctuationPluginKey = new PluginKey<null>(
  "cjkPunctuationGuard",
);

/**
 * 在中文上下文中把 ASCII 标点自动转为全角对应字符。
 *
 * 通过 ProseMirror 的 `handleTextInput` 拦截键入文本：仅当紧邻前一个字符
 * 属于 CJK 上下文时才转换，保护 `1.` 有序列表、URL、英文段落与 markdown
 * 触发符。codeBlock 与 inline code 不转换。IME 合成期 `handleTextInput`
 * 本就不触发，故无需额外处理合成态。
 *
 * 用 `isEnabled` 函数而非布尔：TipTap `configure` 走 `mergeDeep`，对函数当
 * 叶子替换（保留引用），对普通对象会深合并克隆从而断裂引用。调用方传一
 * 个闭包读取稳定的 ref，运行时改 ref.current 即可即时生效，且无需把开关
 * 加入 `useEditor` deps（避免编辑器重建丢失光标/撤销历史）。
 */
export const CjkPunctuationExtension = Extension.create<CjkPunctuationOptions>({
  name: "cjkPunctuation",

  addOptions() {
    return { isEnabled: () => true };
  },

  addProseMirrorPlugins() {
    const isEnabled = this.options.isEnabled;

    return [
      new Plugin({
        key: cjkPunctuationPluginKey,
        props: {
          handleTextInput: (view, from, to, text) => {
            if (!isEnabled()) return false;
            if (text.length !== 1) return false;
            // 早退：非可转换字符直接放行，避免对普通键入做 O(n) 段落扫描
            const isConvertible =
              Object.prototype.hasOwnProperty.call(CJK_PUNCT_MAP, text) ||
              text === '"' ||
              text === "'";
            if (!isConvertible) return false;

            const { state } = view;
            const $from = state.doc.resolve(from);

            // 跳过 codeBlock 与 inline code
            let inCodeBlock = false;
            for (let depth = $from.depth; depth > 0; depth--) {
              if ($from.node(depth).type.name === "codeBlock") {
                inCodeBlock = true;
                break;
              }
            }
            if (inCodeBlock) return false;
            if ($from.marks().some((mark) => mark.type.name === "code")) {
              return false;
            }

            const parent = $from.parent;
            if (!parent.isTextblock) return false;

            const blockText = parent.textContent;
            const beforeChar = blockText.slice(
              Math.max(0, $from.parentOffset - 1),
              $from.parentOffset,
            );

            // beforeChar 非 CJK 上下文时不转换（保护英文/数字/markdown 触发符）
            if (!isCjkContextChar(beforeChar)) return false;

            const { double: unmatchedDouble, single: unmatchedSingle } =
              countUnmatchedSmartQuotes(blockText);
            const result = convertAsciiPunctChar(
              text,
              beforeChar,
              unmatchedDouble,
              unmatchedSingle,
            );
            if (!result.changed) return false;

            const tr = state.tr.insertText(result.converted, from, to);
            view.dispatch(tr);
            return true;
          },
        },
      }),
    ];
  },
});
