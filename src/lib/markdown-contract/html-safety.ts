/** Determine if an HTML token is dangerous (script, object, handlers, URL payloads). */
export function isDangerousHtml(raw: string): boolean {
  const dangerousTags =
    /<\s*\/?\s*(script|object|embed|iframe|form|applet|style|svg|math|link|meta|base|frame|frameset)\b/i;
  if (dangerousTags.test(raw)) return true;
  if (/\son\w+\s*=/i.test(raw)) return true;
  return /\s(?:href|src|xlink:href|formaction|action)\s*=\s*(?:"\s*(?:javascript:|data:text\/html)|'\s*(?:javascript:|data:text\/html)|(?:javascript:|data:text\/html))/i.test(
    raw,
  );
}
