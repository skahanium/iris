import { spawn, spawnSync } from "node:child_process";

const args = process.argv.slice(2);
const env = { ...process.env };
const tauriArgs = [...args];
const devConfig = "src-tauri/tauri.dev.conf.json";
const embeddingFlagIndex = tauriArgs.indexOf("--embedding");
const embeddingsEnabled = args[0] === "dev" && embeddingFlagIndex >= 0;

if (embeddingFlagIndex >= 0) tauriArgs.splice(embeddingFlagIndex, 1);

function hasConfigArg(values) {
  return values.some(
    (value) => value === "--config" || value.startsWith("--config="),
  );
}

if (
  process.platform === "darwin" &&
  args[0] === "dev" &&
  !env.OS_ACTIVITY_MODE
) {
  env.OS_ACTIVITY_MODE = "disable";
}

if (args[0] === "dev" && !hasConfigArg(args)) {
  tauriArgs.push("--config", devConfig);
}

if (embeddingsEnabled) {
  const prepare = spawnSync(
    process.platform === "win32" ? "npm.cmd" : "npm",
    ["run", "model:prepare"],
    {
      cwd: process.cwd(),
      env,
      stdio: "inherit",
    },
  );
  if (prepare.status !== 0) process.exit(prepare.status ?? 1);
  env.IRIS_ENABLE_EMBEDDINGS = "1";
}

const child = spawn("tauri", tauriArgs, {
  env,
  shell: process.platform === "win32",
  stdio: "inherit",
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});

child.on("error", (error) => {
  console.error(error.message);
  process.exit(1);
});
