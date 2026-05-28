import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");
const from = path.join(root, "src-tauri", "icons-staging");
const to = path.join(root, "src-tauri", "icons");

if (!fs.existsSync(from)) {
  console.error(
    "Missing icons-staging. Run: tauri icon scripts/assets/app-icon.png -o src-tauri/icons-staging",
  );
  process.exit(1);
}

function copyDir(src, dest) {
  fs.mkdirSync(dest, { recursive: true });
  for (const name of fs.readdirSync(src, { withFileTypes: true })) {
    const s = path.join(src, name.name);
    const d = path.join(dest, name.name);
    if (name.isDirectory()) {
      copyDir(s, d);
    } else {
      fs.copyFileSync(s, d);
      console.log(`copied ${path.relative(root, d)}`);
    }
  }
}

copyDir(from, to);
console.log(
  "Tauri icons synced. Rebuild desktop app: npm run tauri build -- --debug  or restart tauri dev",
);
