import { useState } from "react";

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
      className="flex flex-col gap-4 p-4"
      data-testid="classified-password-setup"
    >
      <h3 className="text-lg font-semibold">设置涉密保险库密码</h3>
      <p className="text-sm text-muted-foreground">
        设置密码后，`.classified/` 中的文件将被加密保护。
      </p>
      <div className="rounded-md border border-destructive/50 bg-destructive/5 p-3 text-sm text-destructive">
        忘记密码将永久丢失涉密数据，无法恢复。
      </div>
      <Input
        type="password"
        placeholder="输入密码"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        autoComplete="new-password"
      />
      <Input
        type="password"
        placeholder="确认密码"
        value={confirm}
        onChange={(e) => setConfirm(e.target.value)}
        autoComplete="new-password"
        onKeyDown={(e) => {
          if (e.key === "Enter") void handleSubmit();
        }}
      />
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <Button
        type="button"
        onClick={() => void handleSubmit()}
        disabled={loading}
      >
        {loading ? "设置中…" : "确认设置"}
      </Button>
    </div>
  );
}
