import { Moon, Sun } from "lucide-react";

import { PreVaultDesktopFrame } from "@/components/layout/PreVaultDesktopFrame";
import { StartupSplash } from "@/components/layout/StartupSplash";
import { Button } from "@/components/ui/button";

interface AppPreVaultGateProps {
  loading: boolean;
  startupSplashVisible: boolean;
  vaultError: string | null;
  vaultPath: string | null;
  theme: "dark" | "light";
  onExited: () => void;
  onPickVault: () => void;
  onRetryVaultLoad: () => void;
  onThemeChange: (theme: "dark" | "light") => void;
}

export function AppPreVaultGate({
  loading,
  startupSplashVisible,
  vaultError,
  vaultPath,
  theme,
  onExited,
  onPickVault,
  onRetryVaultLoad,
  onThemeChange,
}: AppPreVaultGateProps) {
  if (startupSplashVisible) {
    return (
      <PreVaultDesktopFrame>
        <StartupSplash ready={!loading} onExited={onExited} />
      </PreVaultDesktopFrame>
    );
  }

  if (!vaultPath) {
    return (
      <PreVaultDesktopFrame>
        <VaultPickerScreen
          theme={theme}
          vaultError={vaultError}
          onPickVault={onPickVault}
          onRetryVaultLoad={onRetryVaultLoad}
          onThemeChange={onThemeChange}
        />
      </PreVaultDesktopFrame>
    );
  }

  return null;
}

export function BrowserRuntimeNotice() {
  return (
    <div className="flex h-dvh flex-col items-center justify-center gap-4 bg-background px-6 text-center">
      <h1 className="text-xl font-semibold text-foreground">
        请在 Iris 桌面窗口中使用
      </h1>
      <p className="max-w-md text-sm leading-relaxed text-muted-foreground">
        <code className="rounded bg-muted px-1 py-0.5 text-xs">
          http://127.0.0.1:1420
        </code>{" "}
        这里只是 Vite 前端热更新地址，浏览器里没有 Rust 后端，无法读写笔记目录。
      </p>
      <p className="max-w-md text-sm text-muted-foreground">
        方式 B 需要两个终端：一个 <code className="text-xs">npm run dev</code>
        ，另一个启动 <code className="text-xs">npx tauri dev</code>
        ，请使用弹出的{" "}
        <strong className="font-medium text-foreground">Iris</strong> 窗口操作。
      </p>
    </div>
  );
}

export function VaultPickerScreen({
  theme,
  vaultError,
  onPickVault,
  onRetryVaultLoad,
  onThemeChange,
}: {
  theme: "dark" | "light";
  vaultError: string | null;
  onPickVault: () => void;
  onRetryVaultLoad: () => void;
  onThemeChange: (theme: "dark" | "light") => void;
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-6 bg-background px-6">
      <div className="text-center">
        <h1 className="text-3xl font-semibold tracking-tight text-foreground">
          Iris
        </h1>
        <p className="mt-2 text-sm text-muted-foreground">本地优先笔记</p>
      </div>
      <Button type="button" onClick={onPickVault}>
        选择笔记目录
      </Button>
      {vaultError ? (
        <div className="flex flex-col items-center gap-3">
          <p
            className="max-w-md text-center text-sm text-destructive"
            role="alert"
          >
            {vaultError}
          </p>
          <Button type="button" variant="outline" onClick={onRetryVaultLoad}>
            重试启动检查
          </Button>
        </div>
      ) : null}
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="gap-1.5"
        onClick={() => onThemeChange(theme === "dark" ? "light" : "dark")}
      >
        {theme === "dark" ? (
          <Sun className="h-3.5 w-3.5" />
        ) : (
          <Moon className="h-3.5 w-3.5" />
        )}
        {theme === "dark" ? "亮色模式" : "暗色模式"}
      </Button>
    </div>
  );
}
