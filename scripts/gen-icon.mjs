import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { Resvg } from "@resvg/resvg-js";

import { exportBrandSvgs, monogramSvg } from "./iris-brand-svg.mjs";
import { BRAND_INK } from "./iris-mark-paths.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const assetsDir = path.join(__dirname, "assets");
const brandPublicDir = path.join(__dirname, "..", "public", "brand");

/**
 * @param {string} svg
 * @param {number} width
 * @param {number} [height]
 */
function renderSvgToPng(svg, width, height = width) {
  const resvg = new Resvg(svg, {
    fitTo: { mode: "width", value: width },
  });
  const rendered = resvg.render();
  if (height !== width) {
    const resvgH = new Resvg(svg, {
      fitTo: { mode: "height", value: height },
    });
    return resvgH.render().asPng();
  }
  return rendered.asPng();
}

function writePng(filePath, png) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, png);
  console.log(`wrote ${filePath}`);
}

function writeText(filePath, content) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  console.log(`wrote ${filePath}`);
}

fs.mkdirSync(assetsDir, { recursive: true });
fs.mkdirSync(brandPublicDir, { recursive: true });

const svgs = exportBrandSvgs();

writeText(path.join(brandPublicDir, "iris-mark.svg"), svgs.monogramTransparent);
writeText(path.join(brandPublicDir, "iris-mark-tray.svg"), svgs.monogramTray);
writeText(path.join(brandPublicDir, "iris-mark-app-shell.svg"), svgs.appShell);
writeText(path.join(brandPublicDir, "iris-mark-app-dark.svg"), svgs.appDark);
writeText(path.join(brandPublicDir, "iris-mark-app-light.svg"), svgs.appLight);

/** 桌面安装包 / 任务栏：亮色底、大字号 I（非暗色 in-app 标） */
writePng(
  path.join(assetsDir, "app-icon.png"),
  renderSvgToPng(svgs.appShell, 1024),
);
writePng(
  path.join(assetsDir, "app-icon-dark.png"),
  renderSvgToPng(svgs.appDark, 1024),
);
writePng(
  path.join(assetsDir, "app-icon-light.png"),
  renderSvgToPng(svgs.appLight, 1024),
);

const monoTransparent = monogramSvg({
  frame: BRAND_INK.light.frame,
  ink: BRAND_INK.light.ink,
});
writePng(
  path.join(assetsDir, "iris-mark-transparent.png"),
  renderSvgToPng(monoTransparent, 512),
);

for (const traySize of [16, 22, 32]) {
  writePng(
    path.join(assetsDir, `tray-icon-${traySize}.png`),
    renderSvgToPng(svgs.monogramTray, traySize),
  );
}

writePng(
  path.join(brandPublicDir, "favicon-32.png"),
  renderSvgToPng(monoTransparent, 32),
);

console.log("\nNext: npm run icon:tauri  # 生成 src-tauri/icons/*.ico|.icns");
