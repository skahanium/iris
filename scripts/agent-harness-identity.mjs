/**
 * 登记表身份字段的归一与比对（`P03` §5.2 规则 1／2 的机械部分）。
 *
 * 抽成独立模块有两个原因：一是检查器与它的自测都需要这套判据，重复实现迟早会
 * 分叉；二是两者都有 2000 行预算，而这段逻辑本身与「遍历仓库、报违规」无关。
 *
 * 背景：登记表的 author 字段历史上把「谁做的」与「这轮做了什么」写在一起
 * （`Cursor Grok 4.6 (Package A follow-up)`、`DSH Agent (Q04 F03 self-check)`）。
 * 若按整串比较，同一次收口的作者自审会因为括注不同而被判成两个身份，
 * `P03` §5.2 规则 1 就多出一条只要在括注里改几个字就能绕过的旁路。
 */

/** 文件级容器在权威 JSON 里的登记后缀。 */
export const FILE_OBJECT_SUFFIX = "#file";

export const isContainerRegistration = (id) =>
  String(id ?? "").endsWith(FILE_OBJECT_SUFFIX);

/**
 * 文件级容器的登记键：显式容器 `X#file` 归一化为 `X` 后重新拼，目录登记表直接
 * 给出容器 ID 的单对象文件（`X` 同时是对象 ID 与容器 ID）保持 `X`。
 *
 * 归一化是必需的：`registry.files` 的 `registration` 字段本身可能就是 `X#file`，
 * 直接再拼一次会造出 `X#file#file`——一个既不是对象也不是容器的登记键，任何复核
 * 或变更记录绑上它都会被判「引用未登记对象」。
 */
export const normalizeContainerId = (id) =>
  isContainerRegistration(id)
    ? String(id).slice(0, -FILE_OBJECT_SUFFIX.length)
    : id;

/**
 * 同一实体的历史写法 → 稳定标识。
 *
 * `dsh agent` 与 `deepseek harness` 归并的依据写在 `agent-harness/registry.json`
 * 的 `notes` 里：讨论纪要开篇写明「DeepSeek（批注署名 DSH）」，且环境
 * `DSH_HOME=~/.dsh`，两者是同一实体。`mimo` 是 2026-09-22 登记的独立复核者
 * （MiniMax MiMo），与 `cursor-grok-4.6`／`dsh-agent`／`codex-*` 不是同一实体。
 * `codex-*` 四个署名归并为 `codex-root` 的依据是登记表 2026-09-21 的自述：它们
 * 是同一 root 会话下的子任务路径（`/root`、`/root/budget_implementation`、
 * `/root/session_write_review`、`/root/protocol_search_review`）——同一实体的
 * 不同标签不能互相充当独立复核（2026-09-24 评审 F5）。检查器不猜测实体同一性，
 * 只按本表归并；新增实体必须同步更新登记表的说明。
 */
export const IDENTITY_ALIASES = {
  "dsh-agent": "dsh-agent",
  "dsh agent": "dsh-agent",
  "deepseek harness": "dsh-agent",
  "cursor grok 4.6": "cursor-grok-4.6",
  "cursor-grok-4.6": "cursor-grok-4.6",
  mimo: "mimo",
  skahanium: "skahanium",
  user: "user",
  "codex-root": "codex-root",
  "codex-budget-reviewer": "codex-root",
  "codex-session-reviewer": "codex-root",
  "codex-protocol-reviewer": "codex-root",
};

/**
 * 从作者／复核者字串里取出**实体身份**。
 *
 * 取括注之前的部分（中英文括号都算），再按别名归并。复合署名（如
 * `skahanium / DSH Agent`）表示两个实体共同参与，不当作单一身份。
 */
export function reviewerIdentity(value) {
  const raw = String(value ?? "").trim();
  if (!raw) return null;
  const leading = raw
    .split(/[（(]/, 1)[0]
    .trim()
    .toLowerCase()
    .replace(/^(?:other:\s*)+/, "")
    .trim();
  if (
    /^(?:unattributed|unknown|pending|tbd|待复核|待独立复核|待重验|未署名|未知)$/.test(
      leading,
    )
  )
    return null;
  if (IDENTITY_ALIASES[leading]) return IDENTITY_ALIASES[leading];
  if (leading.startsWith("用户")) return "user";
  return leading
    ? leading.startsWith("other:")
      ? leading
      : `other:${leading}`
    : null;
}

/**
 * 复合署名（`skahanium + dsh-agent`、`skahanium / DSH Agent`）表示多个实体
 * 共同参与。独立性判定必须按**成分集合**求交：任何单一归并键都留有旁路——
 * 作者写复合署名、复核者写其中一个成分，整串比对会误判二者「不同身份」
 * （2026-09-24 评审 F5）。
 */
export function reviewerIdentities(value) {
  const raw = String(value ?? "");
  const parts = raw
    .split(/[+／/&、,]|\s\/\s/)
    .map((part) => part.trim())
    .filter(Boolean);
  const ids = new Set();
  for (const part of parts.length ? parts : [raw]) {
    const id = reviewerIdentity(part);
    if (id) ids.add(id);
  }
  return [...ids];
}

/** True when the two signature strings share at least one entity component. */
export function identitiesOverlap(a, b) {
  const right = new Set(reviewerIdentities(b));
  return reviewerIdentities(a).some((id) => right.has(id));
}

/**
 * 已完成的独立复核。当前对象绑定另由 `reviewCoversCurrentObject` 检查。
 *
 * `reviews[].author` 是复核者，`reviews[].reviewer` 是该变更的编写方（P03 §5.2）。
 * 复核者必须既不等于 `changes[].author`，也不等于 `reviews[].reviewer`。
 * 把 author 与 reviewer 都写成独立身份（例如两者都是 `mimo`）不能解封。
 * 复合署名按成分集合求交比较；`user` 不构成独立复核身份（登记表自述其撰写者
 * 无法核实，2026-09-24 评审 F6 口径）。
 */
export function isIndependentReview(review, change) {
  const author = reviewerIdentities(review?.author);
  if (!author.length || author.includes("user")) return false;
  return Boolean(
    !identitiesOverlap(review.author, change?.author) &&
    !identitiesOverlap(review.author, review.reviewer) &&
    reviewerIdentities(change?.author).length &&
    review.change === change.id &&
    ["no-impact", "synchronized"].includes(review.conclusion) &&
    String(review.reason ?? "").trim().length >= 4 &&
    String(review.evidence ?? "").trim() &&
    review.objects?.some((object) =>
      change.objects?.some((changed) => changed.id === object.id),
    ),
  );
}

/** All bindings, including supplementary reviews, use the same currentness gate. */
export function reviewCoversCurrentObject(review, change, id, fingerprint) {
  return (
    isIndependentReview(review, change) &&
    review.objects.some(
      (binding) =>
        binding.id === id &&
        !binding.stale &&
        fingerprint &&
        binding.fingerprint === fingerprint,
    )
  );
}
