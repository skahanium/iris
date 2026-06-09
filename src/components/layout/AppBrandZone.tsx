import { IrisMark } from "@/components/brand/IrisMark";
import { cn } from "@/lib/utils";

/** 顶栏左侧品牌区（可拖动窗口的握持区域） */
export function AppBrandZone({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "iris-brand-rail flex h-full min-w-[5.5rem] shrink-0 items-center justify-center gap-2.5 border-r border-border/80 px-4",
        className,
      )}
      aria-label="拖动窗口"
    >
      <IrisMark size={20} />
      <span className="text-sm font-semibold tracking-tight text-foreground/90">
        Iris
      </span>
    </div>
  );
}
