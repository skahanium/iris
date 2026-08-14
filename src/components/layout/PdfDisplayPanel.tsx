//! 无业务状态的共享 PDF 显示面板。
//!
//! 调用方负责 URL 的安全 lease、下载与释放；本组件只呈现受控 URL，供笔记库
//! 媒体工作区和 RSS 临时文档复用。

import type { ReactNode } from "react";

export interface PdfDisplayPanelProps {
  url: string;
  label: string;
  testId?: string;
  fallback?: ReactNode;
}

export function PdfDisplayPanel({
  url,
  label,
  testId,
  fallback,
}: PdfDisplayPanelProps) {
  return (
    <object
      data-testid={testId}
      className="h-full min-h-0 w-full bg-background"
      data={url}
      type="application/pdf"
      aria-label={label}
    >
      {fallback ?? (
        <p className="p-4 text-caption text-muted-foreground">
          当前系统无法显示 PDF。
        </p>
      )}
    </object>
  );
}
