import { useState } from "react";

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
      className="flex flex-col gap-4 p-4"
      data-testid="classified-password-prompt"
    >
      <h3 className="text-lg font-semibold">解锁涉密保险库</h3>
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
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <Button
        type="button"
        onClick={() => void handleSubmit()}
        disabled={loading}
      >
        {loading ? "验证中…" : "确认"}
      </Button>
    </div>
  );
}
