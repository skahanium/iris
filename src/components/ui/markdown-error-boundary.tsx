import { Component, type ReactNode } from "react";

interface Props {
  fallback?: ReactNode;
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/**
 * Error boundary for markdown rendering.
 * Catches exceptions thrown during React reconciliation of
 * dangerouslySetInnerHTML content and displays a friendly fallback.
 */
export class MarkdownErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        this.props.fallback ?? (
          <div className="ai-msg text-sm text-muted-foreground italic">
            此消息包含无法渲染的内容。
          </div>
        )
      );
    }
    return this.props.children;
  }
}
