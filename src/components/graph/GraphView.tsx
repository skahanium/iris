import { useCallback, useEffect, useRef, useState } from "react";

import { IrisOverlay } from "@/components/ui/iris-overlay";
import { graphData } from "@/lib/ipc";
import type { GraphData } from "@/types/ipc";
import type {
  GraphLayoutRequest,
  GraphLayoutResponse,
} from "@/workers/graph-layout.worker";

interface GraphViewProps {
  open: boolean;
  onClose: () => void;
  onOpenNote: (path: string) => void | Promise<void>;
  onPrepareNotePath?: (path: string, titleHint?: string) => void;
}

interface SimNode {
  id: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  path: string;
  title: string;
  radius: number;
}

interface SimEdge {
  source: number;
  target: number;
}

interface GraphSimulation {
  nodes: SimNode[];
  edges: SimEdge[];
  nodeById: Map<number, SimNode>;
}

const GRAPH_MAX_ANIMATED_NODES = 140;
const GRAPH_INIT_ITERATIONS = 60;
const GRAPH_LARGE_INIT_ITERATIONS = 20;
const GRAPH_FRAME_ITERATIONS = 3;

function GraphLoadingSkeleton() {
  return (
    <div
      className="absolute inset-0 z-10 flex items-center justify-center"
      aria-live="polite"
      role="status"
      aria-label="知识图谱加载中"
    >
      <div className="relative h-44 w-44 rounded-full border border-border/60 bg-surface-inset/30">
        <span className="absolute left-1/2 top-5 h-3 w-3 -translate-x-1/2 rounded-full bg-muted/70" />
        <span className="absolute bottom-8 left-8 h-4 w-4 rounded-full bg-muted/50" />
        <span className="absolute right-7 top-16 h-5 w-5 rounded-full bg-muted/60" />
        <span className="absolute left-14 top-24 h-px w-20 rotate-12 bg-border" />
        <span className="absolute left-20 top-14 h-px w-16 -rotate-[35deg] bg-border" />
      </div>
    </div>
  );
}

function readCssHsl(varName: string, fallback: string): string {
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue(varName)
    .trim();
  return raw ? `hsl(${raw})` : fallback;
}

function prefersReducedGraphMotion(): boolean {
  return (
    window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false
  );
}

function isGraphAnimationAllowed(sim: GraphSimulation): boolean {
  return (
    !prefersReducedGraphMotion() && sim.nodes.length <= GRAPH_MAX_ANIMATED_NODES
  );
}

function buildSimulation(data: GraphData, width: number, height: number) {
  const maxLinks = Math.max(1, ...data.nodes.map((n) => n.link_count));
  const nodes: SimNode[] = data.nodes.map((n) => ({
    id: n.id,
    x: Math.random() * width,
    y: Math.random() * height,
    vx: 0,
    vy: 0,
    path: n.path,
    title: n.title,
    radius: 6 + (n.link_count / maxLinks) * 16,
  }));
  const edges: SimEdge[] = data.edges.map((e) => ({
    source: e.source,
    target: e.target,
  }));
  const nodeById = new Map(nodes.map((n) => [n.id, n]));
  return { nodes, edges, nodeById } satisfies GraphSimulation;
}

function applyGraphLayout(
  sim: GraphSimulation,
  nodes: GraphLayoutResponse["nodes"],
) {
  for (const layoutNode of nodes) {
    const node = sim.nodeById.get(layoutNode.id);
    if (!node) continue;
    node.x = layoutNode.x;
    node.y = layoutNode.y;
    node.vx = layoutNode.vx;
    node.vy = layoutNode.vy;
  }
}

function drawGraphFrame(
  ctx: CanvasRenderingContext2D,
  canvas: HTMLCanvasElement,
  sim: GraphSimulation,
) {
  const w = canvas.width;
  const h = canvas.height;
  const edgeColor = readCssHsl("--border", "hsl(30 6% 30%)");
  const nodeColor = readCssHsl("--primary", "hsl(28 42% 38%)");
  const labelColor = readCssHsl("--primary-foreground", "hsl(40 33% 94%)");

  ctx.clearRect(0, 0, w, h);
  ctx.strokeStyle = edgeColor;
  ctx.lineWidth = 0.5;
  for (const edge of sim.edges) {
    const src = sim.nodeById.get(edge.source);
    const tgt = sim.nodeById.get(edge.target);
    if (!src || !tgt) continue;
    ctx.beginPath();
    ctx.moveTo(src.x, src.y);
    ctx.lineTo(tgt.x, tgt.y);
    ctx.stroke();
  }

  for (const node of sim.nodes) {
    ctx.fillStyle = nodeColor;
    ctx.beginPath();
    ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
    ctx.fill();

    if (node.radius >= 10) {
      ctx.fillStyle = labelColor;
      ctx.font = `${Math.max(9, node.radius * 0.7)}px sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      const label =
        node.title.length > 6 ? `${node.title.slice(0, 5)}…` : node.title;
      ctx.fillText(label, node.x, node.y);
    }
  }
}

function findNodeAtCanvasEvent(
  canvas: HTMLCanvasElement,
  sim: { nodes: SimNode[] },
  e: React.MouseEvent<HTMLCanvasElement>,
): SimNode | null {
  const rect = canvas.getBoundingClientRect();
  const scaleX = canvas.width / rect.width;
  const scaleY = canvas.height / rect.height;
  const x = (e.clientX - rect.left) * scaleX;
  const y = (e.clientY - rect.top) * scaleY;

  for (const node of sim.nodes) {
    const dx = node.x - x;
    const dy = node.y - y;
    if (Math.sqrt(dx * dx + dy * dy) < node.radius) {
      return node;
    }
  }
  return null;
}

export function GraphView({
  open,
  onClose,
  onOpenNote,
  onPrepareNotePath,
}: GraphViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const simRef = useRef<GraphSimulation | null>(null);
  const animRef = useRef<number>(0);
  const workerRef = useRef<Worker | null>(null);
  const layoutRequestIdRef = useRef(0);
  const layoutResolversRef = useRef(
    new Map<
      number,
      {
        reject: (error: Error) => void;
        resolve: (response: GraphLayoutResponse) => void;
      }
    >(),
  );
  const lastPreparedPathRef = useRef<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [empty, setEmpty] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const getLayoutWorker = useCallback(() => {
    if (workerRef.current) return workerRef.current;
    const worker = new Worker(
      new URL("../../workers/graph-layout.worker.ts", import.meta.url),
      { type: "module" },
    );
    worker.onmessage = (event: MessageEvent<GraphLayoutResponse>) => {
      const resolver = layoutResolversRef.current.get(event.data.requestId);
      if (!resolver) return;
      layoutResolversRef.current.delete(event.data.requestId);
      resolver.resolve(event.data);
    };
    worker.onerror = (event) => {
      const error = new Error(event.message || "图谱布局 worker 失败");
      layoutResolversRef.current.forEach((resolver) => resolver.reject(error));
      layoutResolversRef.current.clear();
    };
    workerRef.current = worker;
    return worker;
  }, []);

  const runGraphLayout = useCallback(
    (sim: GraphSimulation, width: number, height: number, iterations: number) =>
      new Promise<GraphLayoutResponse>((resolve, reject) => {
        const requestId = layoutRequestIdRef.current + 1;
        layoutRequestIdRef.current = requestId;
        layoutResolversRef.current.set(requestId, { resolve, reject });
        const request: GraphLayoutRequest = {
          requestId,
          nodes: sim.nodes,
          edges: sim.edges,
          width,
          height,
          iterations,
        };
        getLayoutWorker().postMessage(request);
      }),
    [getLayoutWorker],
  );

  const resizeCanvas = useCallback(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) return { w: 800, h: 600 };
    const w = container.clientWidth;
    const h = container.clientHeight;
    canvas.width = w;
    canvas.height = h;
    return { w, h };
  }, []);

  const paintCurrentGraph = useCallback(() => {
    const canvas = canvasRef.current;
    const sim = simRef.current;
    if (!canvas || !sim) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    drawGraphFrame(ctx, canvas, sim);
  }, []);

  const initGraph = useCallback(async () => {
    setLoading(true);
    setError(null);
    setEmpty(false);
    try {
      const data = await graphData();
      if (data.nodes.length === 0) {
        simRef.current = null;
        setEmpty(true);
        return;
      }
      const { w, h } = resizeCanvas();
      const sim = buildSimulation(data, w, h);
      const initIterations =
        sim.nodes.length > GRAPH_MAX_ANIMATED_NODES
          ? GRAPH_LARGE_INIT_ITERATIONS
          : GRAPH_INIT_ITERATIONS;

      const layout = await runGraphLayout(sim, w, h, initIterations);
      applyGraphLayout(sim, layout.nodes);

      simRef.current = sim;
      setEmpty(false);
      paintCurrentGraph();
    } catch (e) {
      setError(e instanceof Error ? e.message : "加载图谱失败");
      simRef.current = null;
      setEmpty(false);
    } finally {
      setLoading(false);
    }
  }, [paintCurrentGraph, resizeCanvas, runGraphLayout]);

  const startAnimation = useCallback(() => {
    const canvas = canvasRef.current;
    const sim = simRef.current;
    if (!canvas || !sim) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    drawGraphFrame(ctx, canvas, sim);
    if (!isGraphAnimationAllowed(sim)) return;

    let running = true;
    let idleFrames = 0;

    const tick = () => {
      if (!running) return;
      const current = simRef.current;
      if (!current) return;

      void runGraphLayout(
        current,
        canvas.width,
        canvas.height,
        GRAPH_FRAME_ITERATIONS,
      )
        .then((layout) => {
          if (!running) return;
          applyGraphLayout(current, layout.nodes);
          drawGraphFrame(ctx, canvas, current);

          const maxSpeed = current.nodes.reduce(
            (max, n) => Math.max(max, Math.hypot(n.vx, n.vy)),
            0,
          );
          if (maxSpeed < 0.08) {
            idleFrames += 1;
          } else {
            idleFrames = 0;
          }
          if (idleFrames >= 45) {
            running = false;
            return;
          }

          animRef.current = requestAnimationFrame(tick);
        })
        .catch(() => {
          running = false;
        });
    };

    animRef.current = requestAnimationFrame(tick);
    return () => {
      running = false;
    };
  }, [runGraphLayout]);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const canvas = canvasRef.current;
      const sim = simRef.current;
      if (!canvas || !sim) return;
      const node = findNodeAtCanvasEvent(canvas, sim, e);
      if (!node) {
        lastPreparedPathRef.current = null;
        return;
      }
      if (lastPreparedPathRef.current === node.path) return;
      lastPreparedPathRef.current = node.path;
      onPrepareNotePath?.(node.path, node.title);
    },
    [onPrepareNotePath],
  );

  const handleMouseLeave = useCallback(() => {
    lastPreparedPathRef.current = null;
  }, []);

  const handleClick = useCallback(
    async (e: React.MouseEvent<HTMLCanvasElement>) => {
      const canvas = canvasRef.current;
      const sim = simRef.current;
      if (!canvas || !sim) return;
      const node = findNodeAtCanvasEvent(canvas, sim, e);
      if (!node) return;
      await onOpenNote(node.path);
      onClose();
    },
    [onClose, onOpenNote],
  );

  useEffect(() => {
    if (!open) {
      cancelAnimationFrame(animRef.current);
      workerRef.current?.terminate();
      workerRef.current = null;
      layoutResolversRef.current.clear();
      simRef.current = null;
      setEmpty(false);
      return;
    }

    let stopAnim: (() => void) | undefined;
    void initGraph().then(() => {
      stopAnim = startAnimation();
    });

    const container = containerRef.current;
    if (!container) {
      return () => {
        stopAnim?.();
        cancelAnimationFrame(animRef.current);
      };
    }

    const ro = new ResizeObserver(() => {
      resizeCanvas();
      paintCurrentGraph();
    });
    ro.observe(container);
    const layoutResolvers = layoutResolversRef.current;

    return () => {
      ro.disconnect();
      stopAnim?.();
      cancelAnimationFrame(animRef.current);
      workerRef.current?.terminate();
      workerRef.current = null;
      layoutResolvers.clear();
    };
  }, [open, initGraph, startAnimation, resizeCanvas, paintCurrentGraph]);

  return (
    <IrisOverlay
      open={open}
      onClose={onClose}
      title="知识图谱"
      size="graph"
      bodyClassName="relative"
    >
      {error && (
        <p className="task-overlay-filter border-b border-border px-3 py-2 text-xs text-destructive">
          {error}
        </p>
      )}
      {loading && <GraphLoadingSkeleton />}
      {empty ? (
        <div
          className="absolute inset-0 z-10 flex flex-col items-center justify-center gap-1 text-center text-sm text-muted-foreground"
          role="status"
        >
          <div className="font-medium text-foreground">图谱暂无节点</div>
          <div className="text-xs">创建双向链接后，这里会显示知识关系。</div>
        </div>
      ) : null}
      <div
        ref={containerRef}
        className="task-overlay-results relative min-h-0 flex-1"
      >
        <canvas
          ref={canvasRef}
          className="h-full w-full cursor-pointer"
          onMouseMove={handleMouseMove}
          onMouseLeave={handleMouseLeave}
          onClick={(event) => void handleClick(event)}
        />
      </div>
    </IrisOverlay>
  );
}

export type { GraphNode } from "@/types/ipc";
