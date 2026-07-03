declare module "node:fs" {
  export interface Dirent {
    name: string;
    isDirectory(): boolean;
  }

  export function readdirSync(
    path: string,
    options: { withFileTypes: true },
  ): Dirent[];
  export function readFileSync(path: string, encoding: "utf8"): string;
  export function writeFileSync(
    path: string,
    data: string,
    encoding: "utf8",
  ): void;
  export function existsSync(path: string): boolean;
  export function mkdirSync(
    path: string,
    options?: { recursive?: boolean },
  ): void;
  export function mkdtempSync(prefix: string): string;
  export function rmSync(
    path: string,
    options?: { recursive?: boolean; force?: boolean },
  ): void;
}

declare module "node:path" {
  export function dirname(path: string): string;
  export function join(...paths: string[]): string;
  export function resolve(...paths: string[]): string;
}

declare module "node:os" {
  export function tmpdir(): string;
}

declare module "node:child_process" {
  export interface SpawnSyncResult {
    status: number | null;
    stdout: string;
    stderr: string;
  }

  export function spawnSync(
    file: string,
    args: string[],
    options: {
      cwd?: string;
      encoding: "utf8";
      env?: Record<string, string | undefined>;
    },
  ): SpawnSyncResult;

  export function execFileSync(
    file: string,
    args: string[],
    options: { encoding: "utf8" },
  ): string;
}

declare const __dirname: string;
declare const process: {
  cwd(): string;
  execPath: string;
  env: {
    HOME?: string;
    [key: string]: string | undefined;
  };
};
