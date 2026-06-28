interface DocumentOpenLoadingSurfaceProps {
  path: string;
  title?: string | null;
  zen?: boolean;
}

function fileNameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function DocumentOpenLoadingSurface({
  path,
  title,
  zen = false,
}: DocumentOpenLoadingSurfaceProps) {
  const displayTitle = title?.trim() || fileNameFromPath(path);

  return (
    <div
      data-testid="document-open-loading"
      className="iris-editor iris-document-open-loading"
      data-zen={zen ? "true" : undefined}
      aria-live="polite"
      aria-busy="true"
    >
      <div className="iris-editor-zoom-scroll min-h-0 flex-1 overflow-hidden">
        <div className="iris-editor-canvas">
          <div className="iris-document-open-loading-header">
            <div
              className="iris-document-open-loading-title"
              title={displayTitle}
            >
              {displayTitle}
            </div>
            <div className="iris-document-open-loading-status">正在打开</div>
          </div>
          <div className="iris-document-open-loading-skeleton" aria-hidden>
            <div className="iris-document-open-loading-line is-heading" />
            <div className="iris-document-open-loading-line is-wide" />
            <div className="iris-document-open-loading-line" />
            <div className="iris-document-open-loading-line is-medium" />
            <div className="iris-document-open-loading-line is-wide" />
            <div className="iris-document-open-loading-line is-short" />
            <div className="iris-document-open-loading-quote">
              <div className="iris-document-open-loading-line is-medium" />
              <div className="iris-document-open-loading-line is-short" />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
