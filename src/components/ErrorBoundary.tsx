import { Component, type ErrorInfo, type ReactNode } from "react";

import { Button } from "@/components/ui/button";

interface Props {
  children: ReactNode;
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
    console.error("Iris render error:", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex h-screen flex-col items-center justify-center gap-4 bg-background px-6 text-center">
          <h1 className="text-lg font-semibold text-foreground">
            界面出现异常
          </h1>
          <p className="max-w-md text-sm text-muted-foreground">
            {this.state.error.message}
          </p>
          <Button type="button" onClick={() => window.location.reload()}>
            重新加载
          </Button>
        </div>
      );
    }
    return this.props.children;
  }
}
