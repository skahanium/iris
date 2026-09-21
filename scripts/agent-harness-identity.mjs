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
 * `DSH_HOME=~/.dsh`，两者是同一实体。检查器不猜测实体同一性，只按本表归并；
 * 新增实体必须同步更新登记表的说明。
 */
export const IDENTITY_ALIASES = {
  "dsh agent": "dsh-agent",
  "deepseek harness": "dsh-agent",
  "cursor grok 4.6": "cursor-grok-4.6",
  skahanium: "skahanium",
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
  const leading = raw.split(/[（(]/, 1)[0].trim().toLowerCase();
  if (IDENTITY_ALIASES[leading]) return IDENTITY_ALIASES[leading];
  if (leading.startsWith("用户")) return "user";
  return leading ? `other:${leading}` : null;
}
