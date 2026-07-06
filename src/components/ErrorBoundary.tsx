import { Component, Fragment, type ErrorInfo, type ReactNode } from "react";

import { Button } from "@/components/ui/button";

interface Props {
  children: ReactNode;
  scope?: string;
}

interface CrashDiagnostic {
  componentStack: string[];
  errorName: string;
  messageHash: string;
  messageLength: number;
  scope: string | null;
  timestamp: string;
}

interface State {
  copyStatus: "idle" | "copied" | "failed";
  diagnostic: CrashDiagnostic | null;
  error: Error | null;
  resetVersion: number;
}

function hashMessage(message: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < message.length; index += 1) {
    hash ^= message.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `h${hash.toString(16).padStart(8, "0")}`;
}

function summarizeComponentStack(componentStack?: string | null): string[] {
  return (componentStack ?? "")
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, 6);
}

function createCrashDiagnostic(
  error: Error,
  info: ErrorInfo,
  scope?: string,
): CrashDiagnostic {
  const message = error.message ?? "";
  return {
    componentStack: summarizeComponentStack(info.componentStack),
    errorName: error.name || "Error",
    messageHash: hashMessage(message),
    messageLength: message.length,
    scope: scope ?? null,
    timestamp: new Date().toISOString(),
  };
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = {
    copyStatus: "idle",
    diagnostic: null,
    error: null,
    resetVersion: 0,
  };

  static getDerivedStateFromError(error: Error): Pick<State, "error"> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    const diagnostic = createCrashDiagnostic(error, info, this.props.scope);
    this.setState({ copyStatus: "idle", diagnostic });
    console.error("Iris render error:", diagnostic);
  }

  private handleRetry = () => {
    this.setState((prev) => ({
      copyStatus: "idle",
      diagnostic: null,
      error: null,
      resetVersion: prev.resetVersion + 1,
    }));
  };

  private handleCopyDiagnostics = () => {
    const { diagnostic } = this.state;
    if (!diagnostic) return;

    const clipboard = navigator.clipboard;
    if (!clipboard) {
      this.setState({ copyStatus: "failed" });
      return;
    }

    void clipboard
      .writeText(JSON.stringify(diagnostic, null, 2))
      .then(() => this.setState({ copyStatus: "copied" }))
      .catch(() => this.setState({ copyStatus: "failed" }));
  };

  render() {
    if (this.state.error) {
      return (
        <div className="flex flex-col items-center justify-center gap-3 p-6 text-center">
          <h1 className="text-sm font-semibold text-foreground">
            界面出现异常{this.props.scope ? `（${this.props.scope}）` : ""}
          </h1>
          <p className="max-w-md text-xs text-muted-foreground">
            {this.state.error.message}
          </p>
          <div className="flex flex-wrap items-center justify-center gap-2">
            <Button type="button" size="sm" onClick={this.handleRetry}>
              重试
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              data-testid="error-boundary-copy-diagnostics"
              onClick={this.handleCopyDiagnostics}
            >
              复制诊断
            </Button>
          </div>
          {this.state.copyStatus !== "idle" ? (
            <p className="text-[11px] text-muted-foreground">
              {this.state.copyStatus === "copied"
                ? "诊断信息已复制"
                : "诊断信息复制失败"}
            </p>
          ) : null}
        </div>
      );
    }
    return (
      <Fragment key={this.state.resetVersion}>{this.props.children}</Fragment>
    );
  }
}
