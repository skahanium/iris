/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_SKIP_AUTO_INDEX?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
