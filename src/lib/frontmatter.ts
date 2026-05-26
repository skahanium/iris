/** Obsidian-style `---` YAML frontmatter (subset parser for Iris notes). */

export interface SplitFrontmatterResult {
  /** Raw YAML between fences, without `---` lines; null if absent. */
  yaml: string | null;
  fields: Record<string, string | string[]>;
  body: string;
}

function unquoteYamlScalar(raw: string): string {
  const t = raw.trim();
  if (
    (t.startsWith('"') && t.endsWith('"')) ||
    (t.startsWith("'") && t.endsWith("'"))
  ) {
    const inner = t.slice(1, -1);
    return t.startsWith('"')
      ? inner.replace(/\\"/g, '"').replace(/\\\\/g, "\\")
      : inner;
  }
  return t;
}

function parseYamlArray(raw: string): string[] {
  const inner = raw.trim().slice(1, -1).trim();
  if (!inner) return [];
  return inner.split(",").map((s) => unquoteYamlScalar(s.trim()));
}

/** Parse `key: value` lines from a frontmatter block. */
export function parseYamlFields(yaml: string): Record<string, string | string[]> {
  const fields: Record<string, string | string[]> = {};
  for (const line of yaml.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const m = /^([a-zA-Z_][\w-]*)\s*:\s*(.*)$/.exec(trimmed);
    if (!m) continue;
    const key = m[1]!;
    const raw = m[2]!;
    if (raw.trimStart().startsWith("[")) {
      fields[key] = parseYamlArray(raw);
    } else {
      fields[key] = unquoteYamlScalar(raw);
    }
  }
  return fields;
}

/** Split leading frontmatter from markdown body. */
export function splitFrontmatter(content: string): SplitFrontmatterResult {
  const normalized = content.replace(/^\uFEFF/, "");
  if (!normalized.startsWith("---")) {
    return { yaml: null, fields: {}, body: normalized };
  }
  const end = normalized.indexOf("\n---", 3);
  if (end === -1) {
    return { yaml: null, fields: {}, body: normalized };
  }
  const yaml = normalized.slice(4, end).trim();
  const body = normalized.slice(end + 4).replace(/^\n/, "");
  return { yaml: yaml || null, fields: parseYamlFields(yaml), body };
}

/** Quote a YAML scalar (always double-quoted for safety). */
export function quoteYamlString(value: string): string {
  const escaped = value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `"${escaped}"`;
}

/**
 * Build full note markdown: frontmatter (preserving non-title fields) + body.
 */
export function serializeNoteMarkdown(
  existingYaml: string | null,
  title: string,
  bodyMarkdown: string,
): string {
  const lines = existingYaml ? existingYaml.split("\n") : [];
  const titleLine = `title: ${quoteYamlString(title)}`;
  let foundTitle = false;
  const out: string[] = [];
  for (const line of lines) {
    if (/^title\s*:/.test(line.trim())) {
      foundTitle = true;
      out.push(titleLine);
    } else {
      out.push(line);
    }
  }
  if (!foundTitle) {
    out.unshift(titleLine);
  }
  const yamlBlock = out.join("\n").trim();
  const body = bodyMarkdown.trimEnd();
  return `---\n${yamlBlock}\n---\n\n${body ? `${body}\n` : ""}`;
}

/** Read display title from parsed frontmatter fields. */
export function titleFromFields(
  fields: Record<string, string | string[]>,
): string {
  const t = fields.title;
  if (typeof t === "string") return t.trim();
  return "";
}
