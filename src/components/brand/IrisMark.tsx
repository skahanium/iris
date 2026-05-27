import { cn } from "@/lib/utils";

import {
  FRAME,
  I_BOTTOM,
  I_SKEW,
  I_STEM_PATH,
  I_TOP,
  MARK_VIEWBOX,
} from "./iris-mark-paths";

export interface IrisMarkProps {
  size?: number;
  className?: string;
  title?: string;
}

function MarkRect({
  r,
  className,
}: {
  r: { x: number; y: number; w: number; h: number; rx: number };
  className: string;
}) {
  return (
    <rect
      x={r.x}
      y={r.y}
      width={r.w}
      height={r.h}
      rx={r.rx}
      className={className}
    />
  );
}

/** 几何 monogram「I」：圆角框 + 斜切衬线 I（顶栏、favicon、应用图标） */
export function IrisMark({ size = 20, className, title }: IrisMarkProps) {
  const labelled = title
    ? { role: "img" as const, "aria-label": title }
    : { "aria-hidden": true };

  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox={MARK_VIEWBOX}
      width={size}
      height={size}
      className={cn("shrink-0", className)}
      {...labelled}
    >
      <MarkRect r={FRAME} className="fill-[hsl(var(--iris-mark-frame))]" />
      <g transform={`translate(16 16) skewX(${I_SKEW}) translate(-16 -16)`}>
        <MarkRect r={I_TOP} className="fill-[hsl(var(--iris-mark-ink))]" />
        <path d={I_STEM_PATH} className="fill-[hsl(var(--iris-mark-ink))]" />
        <MarkRect r={I_BOTTOM} className="fill-[hsl(var(--iris-mark-ink))]" />
      </g>
    </svg>
  );
}
