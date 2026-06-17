import { useState } from "react";
import { ShieldCheck } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { classifiedSetup } from "@/lib/ipc";

interface ClassifiedPasswordSetupProps {
  onSuccess: () => void;
}

export function ClassifiedPasswordSetup({
  onSuccess,
}: ClassifiedPasswordSetupProps) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async () => {
    setError("");
    if (password.length < 1) {
      setError("请输入密码");
      return;
    }
    if (password !== confirm) {
      setError("两次输入不一致");
      return;
    }
    setLoading(true);
    try {
      await classifiedSetup(password);
      setPassword("");
      setConfirm("");
      onSuccess();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "设置失败");
      setPassword("");
      setConfirm("");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div
      className="flex min-h-[24rem] flex-col justify-center gap-4 p-6"
      data-testid="classified-password-setup"
    >
      <div className="flex items-start gap-3">
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg border border-border/60 bg-surface-inset text-muted-foreground">
          <ShieldCheck className="h-5 w-5" />
        </div>
        <div className="min-w-0 space-y-1">
          <h3 className="text-lg font-semibold">设置保险库密码</h3>
          <p className="text-sm text-muted-foreground">
            设置后，涉密文件会在本机加密保存。
          </p>
        </div>
      </div>
      <div className="rounded-lg border border-destructive/50 bg-destructive/5 p-3 text-sm text-destructive">
        忘记密码将永久丢失涉密数据，无法恢复。
      </div>
      <label className="grid gap-2 text-sm">
        <span className="text-muted-foreground">新密码</span>
        <Input
          type="password"
          placeholder="输入密码"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="new-password"
        />
      </label>
      <label className="grid gap-2 text-sm">
        <span className="text-muted-foreground">确认密码</span>
        <Input
          type="password"
          placeholder="再次输入密码"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          autoComplete="new-password"
          onKeyDown={(e) => {
            if (e.key === "Enter") void handleSubmit();
          }}
        />
      </label>
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <Button
        type="button"
        onClick={() => void handleSubmit()}
        disabled={loading}
        className="self-start"
      >
        {loading ? "设置中…" : "设置密码"}
      </Button>
    </div>
  );
}
