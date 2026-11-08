import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  /** Changing this value clears the error (e.g. the current workspace id). */
  resetKey?: unknown;
  title?: string;
}

interface State {
  error: Error | null;
  resetKey: unknown;
}

/** Shows render errors instead of an empty window. */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, resetKey: this.props.resetKey };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  static getDerivedStateFromProps(props: Props, state: State): Partial<State> | null {
    return props.resetKey !== state.resetKey ? { error: null, resetKey: props.resetKey } : null;
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("UI error:", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div
        role="alert"
        className="m-4 rounded-md border border-danger/50 bg-danger/10 p-4 text-sm"
        style={{ color: "var(--text, #d7dee8)" }}
      >
        <div className="mb-1 font-semibold">{this.props.title ?? "Something went wrong in this view"}</div>
        <pre className="selectable overflow-auto text-xs whitespace-pre-wrap opacity-80">{error.message}</pre>
        <button
          type="button"
          className="mt-3 rounded border border-line px-3 py-1 text-xs hover:bg-raised"
          onClick={() => this.setState({ error: null })}
        >
          Try again
        </button>
      </div>
    );
  }
}
