import { existsSync, mkdirSync } from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const devHome = path.join(root, ".iris-dev");

function buildIrisEnv(baseEnv = process.env) {
  const irisHome = baseEnv.IRIS_HOME || devHome;
  const irisCache = baseEnv.IRIS_CACHE_DIR || path.join(irisHome, "cache");
  const irisTemp = baseEnv.IRIS_TEMP_DIR || path.join(irisHome, "tmp");
  const irisConfig = baseEnv.IRIS_CONFIG_DIR || path.join(irisHome, "config");
  const env = {
    ...baseEnv,
    IRIS_HOME: irisHome,
    IRIS_DATA_DIR: baseEnv.IRIS_DATA_DIR || path.join(irisHome, "app-data"),
    IRIS_CONFIG_DIR: irisConfig,
    IRIS_CACHE_DIR: irisCache,
    IRIS_TEMP_DIR: irisTemp,
    IRIS_GLOBAL_SKILLS_DIR:
      baseEnv.IRIS_GLOBAL_SKILLS_DIR || path.join(irisHome, "skills"),
    npm_config_cache: path.join(irisCache, "npm"),
    CARGO_TARGET_DIR: path.join(irisHome, "target"),
    ORT_CACHE_DIR: path.join(irisCache, "ort"),
    HF_HOME: path.join(irisCache, "huggingface"),
    HF_HUB_CACHE: path.join(irisCache, "huggingface", "hub"),
    XDG_CACHE_HOME: path.join(irisCache, "xdg"),
    TEMP: irisTemp,
    TMP: irisTemp,
    TMPDIR: irisTemp,
  };

  return env;
}

function ensureIrisDirs(env) {
  for (const key of [
    "IRIS_HOME",
    "IRIS_DATA_DIR",
    "IRIS_CONFIG_DIR",
    "IRIS_CACHE_DIR",
    "IRIS_TEMP_DIR",
    "IRIS_GLOBAL_SKILLS_DIR",
    "npm_config_cache",
    "CARGO_TARGET_DIR",
    "ORT_CACHE_DIR",
    "HF_HOME",
    "HF_HUB_CACHE",
    "XDG_CACHE_HOME",
  ]) {
    mkdirSync(env[key], { recursive: true });
  }
}

const args = process.argv.slice(2);
const env = buildIrisEnv();
ensureIrisDirs(env);

if (args[0] === "--print-env") {
  const keys = [
    "IRIS_HOME",
    "IRIS_DATA_DIR",
    "IRIS_CONFIG_DIR",
    "IRIS_CACHE_DIR",
    "IRIS_TEMP_DIR",
    "IRIS_GLOBAL_SKILLS_DIR",
    "npm_config_cache",
    "CARGO_TARGET_DIR",
    "ORT_CACHE_DIR",
    "HF_HOME",
    "HF_HUB_CACHE",
    "XDG_CACHE_HOME",
    "TEMP",
    "TMP",
    "TMPDIR",
  ];
  const printable = Object.fromEntries(keys.map((key) => [key, env[key]]));
  process.stdout.write(`${JSON.stringify(printable, null, 2)}\n`);
  process.exit(0);
}

const separator = args[0] === "--" ? 1 : 0;
const command = args[separator];
const commandArgs = args.slice(separator + 1);

function resolveCommand(commandName) {
  const localBin = path.join(
    root,
    "node_modules",
    ".bin",
    process.platform === "win32" ? `${commandName}.cmd` : commandName,
  );
  return existsSync(localBin) ? localBin : commandName;
}

if (!command) {
  console.error(
    "Usage: node scripts/with-iris-env.mjs [--print-env | -- <command> [...args]]",
  );
  process.exit(2);
}

const resolvedCommand = resolveCommand(command);
const useShell =
  process.platform === "win32" && resolvedCommand.endsWith(".cmd");
const shellCommand = useShell
  ? [resolvedCommand, ...commandArgs]
      .map((part) => `"${part.replaceAll('"', '\\"')}"`)
      .join(" ")
  : resolvedCommand;
const child = spawn(shellCommand, useShell ? [] : commandArgs, {
  cwd: root,
  env,
  shell: useShell,
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
