import { Component, type ErrorInfo, type ReactNode } from "react";

import { Button } from "@/components/ui/button";

interface Props {
  children: ReactNode;
  scope?: string;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(
      `Iris render error${this.props.scope ? ` [${this.props.scope}]` : ""}:`,
      error,
      info.componentStack,
    );
  }

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
          <Button
            type="button"
            size="sm"
            onClick={() => this.setState({ error: null })}
          >
            重试
          </Button>
        </div>
      );
    }
    return this.props.children;
  }
}
