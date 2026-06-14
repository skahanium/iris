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
}

declare module "node:path" {
  export function join(...paths: string[]): string;
  export function resolve(...paths: string[]): string;
}

declare module "node:child_process" {
  export function execFileSync(
    file: string,
    args: string[],
    options: { encoding: "utf8" },
  ): string;
}

declare const __dirname: string;
declare const process: {
  cwd(): string;
  env: {
    HOME?: string;
  };
};
