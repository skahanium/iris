import { useState } from "react";
import { LockKeyhole } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { classifiedUnlock } from "@/lib/ipc";

interface ClassifiedPasswordPromptProps {
  onSuccess: () => void;
}

export function ClassifiedPasswordPrompt({
  onSuccess,
}: ClassifiedPasswordPromptProps) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async () => {
    setError("");
    if (!password) {
      setError("请输入密码");
      return;
    }
    setLoading(true);
    try {
      await classifiedUnlock(password);
      setPassword("");
      onSuccess();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "解锁失败");
      setPassword("");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div
      className="flex min-h-[22rem] flex-col justify-center gap-4 p-6"
      data-testid="classified-password-prompt"
    >
      <div className="flex items-start gap-3">
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg border border-border/60 bg-surface-inset text-muted-foreground">
          <LockKeyhole className="h-5 w-5" />
        </div>
        <div className="min-w-0 space-y-1">
          <h3 className="text-lg font-semibold">解锁保险库</h3>
          <p className="text-sm text-muted-foreground">
            输入密码后才能查看和编辑涉密文件。
          </p>
        </div>
      </div>
      <label className="grid gap-2 text-sm">
        <span className="text-muted-foreground">保险库密码</span>
        <Input
          type="password"
          placeholder="输入密码"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void handleSubmit();
          }}
          autoFocus
          autoComplete="current-password"
        />
      </label>
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <Button
        type="button"
        onClick={() => void handleSubmit()}
        disabled={loading}
        className="self-start"
      >
        {loading ? "验证中…" : "解锁"}
      </Button>
    </div>
  );
}
