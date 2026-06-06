/** Vault-relative path under `.classified/` (not the directory root itself). */
export function isClassifiedVaultPath(path: string): boolean {
  const normalized = path.replace(/\\/g, "/");
  return normalized.startsWith(".classified/") && normalized !== ".classified";
}

/** Convert an absolute filesystem path to a vault-relative path, if under `vaultPath`. */
export function vaultRelativePath(
  vaultPath: string,
  absolutePath: string,
): string | null {
  const normVault = vaultPath.replace(/\\/g, "/").replace(/\/$/, "");
  const normAbs = absolutePath.replace(/\\/g, "/");
  const prefix = `${normVault}/`;
  if (!normAbs.startsWith(prefix)) return null;
  return normAbs.slice(prefix.length);
}

/** Reject internal vault paths for classified import source selection. */
export function isImportableUserNotePath(path: string): boolean {
  const normalized = path.replace(/\\/g, "/");
  if (normalized.startsWith(".iris/") || normalized === ".iris") return false;
  if (normalized.startsWith(".classified/") || normalized === ".classified") {
    return false;
  }
  return normalized.length > 0 && !normalized.includes("..");
}
