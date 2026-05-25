import { useCallback, useEffect, useRef } from "react";

import { Button } from "@/components/ui/button";
import { graphData } from "@/lib/ipc";
import type { GraphData, GraphNode } from "@/types/ipc";

interface GraphViewProps {
  open: boolean;
  onClose: () => void;
  onOpenNote: (path: string) => void;
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

function forceSimulate(
  nodes: SimNode[],
  edges: SimEdge[],
  width: number,
  height: number,
  iterations: number,
) {
  const cx = width / 2;
  const cy = height / 2;
  const kRepel = 5000;
  const kAttract = 0.01;
  const damping = 0.85;
  const maxSpeed = 5;

  for (let iter = 0; iter < iterations; iter++) {
    // Repulsion between all node pairs
    for (let i = 0; i < nodes.length; i++) {
      for (let j = i + 1; j < nodes.length; j++) {
        const a = nodes[i];
        const b = nodes[j];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        const dist = Math.sqrt(dx * dx + dy * dy) || 1;
        const force = kRepel / (dist * dist);
        const fx = (dx / dist) * force;
        const fy = (dy / dist) * force;
        a.vx -= fx;
        a.vy -= fy;
        b.vx += fx;
        b.vy += fy;
      }
    }

    // Attraction along edges
    for (const edge of edges) {
      const src = nodes.find((n) => n.id === edge.source);
      const tgt = nodes.find((n) => n.id === edge.target);
      if (!src || !tgt) continue;
      const dx = tgt.x - src.x;
      const dy = tgt.y - src.y;
      const dist = Math.sqrt(dx * dx + dy * dy) || 1;
      const force = dist * kAttract;
      const fx = (dx / dist) * force;
      const fy = (dy / dist) * force;
      src.vx += fx;
      src.vy += fy;
      tgt.vx -= fx;
      tgt.vy -= fy;
    }

    // Centering
    for (const node of nodes) {
      node.vx += (cx - node.x) * 0.001;
      node.vy += (cy - node.y) * 0.001;
    }

    // Apply velocity with damping
    for (const node of nodes) {
      node.vx *= damping;
      node.vy *= damping;
      const speed = Math.sqrt(node.vx * node.vx + node.vy * node.vy);
      if (speed > maxSpeed) {
        node.vx = (node.vx / speed) * maxSpeed;
        node.vy = (node.vy / speed) * maxSpeed;
      }
      node.x += node.vx;
      node.y += node.vy;
      node.x = Math.max(node.radius, Math.min(width - node.radius, node.x));
      node.y = Math.max(node.radius, Math.min(height - node.radius, node.y));
    }
  }
}

export function GraphView({ open, onClose, onOpenNote }: GraphViewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const simRef = useRef<{ nodes: SimNode[]; edges: SimEdge[] } | null>(null);
  const animRef = useRef<number>(0);

  const initGraph = useCallback(async () => {
    const data: GraphData = await graphData();
    if (data.nodes.length === 0) return;

    const w = canvasRef.current?.width ?? 800;
    const h = canvasRef.current?.height ?? 600;

    const maxLinks = Math.max(1, ...data.nodes.map((n) => n.link_count));
    const nodes: SimNode[] = data.nodes.map((n) => ({
      id: n.id,
      x: Math.random() * w,
      y: Math.random() * h,
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

    // Initial simulation
    forceSimulate(nodes, edges, w, h, 200);

    simRef.current = { nodes, edges };
    draw();
  }, []);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const sim = simRef.current;
    if (!sim) return;

    const w = canvas.width;
    const h = canvas.height;
    ctx.clearRect(0, 0, w, h);

    // Edges
    ctx.strokeStyle = "hsl(30 6% 30%)";
    ctx.lineWidth = 0.5;
    for (const edge of sim.edges) {
      const src = sim.nodes.find((n) => n.id === edge.source);
      const tgt = sim.nodes.find((n) => n.id === edge.target);
      if (!src || !tgt) continue;
      ctx.beginPath();
      ctx.moveTo(src.x, src.y);
      ctx.lineTo(tgt.x, tgt.y);
      ctx.stroke();
    }

    // Nodes
    for (const node of sim.nodes) {
      ctx.fillStyle = "hsl(28 42% 38%)";
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
      ctx.fill();

      // Label for larger nodes
      if (node.radius >= 10) {
        ctx.fillStyle = "hsl(40 33% 94%)";
        ctx.font = `${Math.max(9, node.radius * 0.7)}px sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        const label = node.title.length > 6 ? node.title.slice(0, 5) + "…" : node.title;
        ctx.fillText(label, node.x, node.y);
      }
    }

    // Continue simulation
    forceSimulate(sim.nodes, sim.edges, w, h, 3);
    animRef.current = requestAnimationFrame(draw);
  }, []);

  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const rect = canvas.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      const sim = simRef.current;
      if (!sim) return;

      for (const node of sim.nodes) {
        const dx = node.x - x;
        const dy = node.y - y;
        if (Math.sqrt(dx * dx + dy * dy) < node.radius) {
          onOpenNote(node.path);
          break;
        }
      }
    },
    [onOpenNote],
  );

  useEffect(() => {
    if (!open) {
      cancelAnimationFrame(animRef.current);
      simRef.current = null;
      return;
    }
    void initGraph();
    return () => cancelAnimationFrame(animRef.current);
  }, [open, initGraph]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-background/95">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-sm font-medium">知识图谱</span>
        <Button type="button" size="sm" variant="ghost" onClick={onClose}>
          Esc
        </Button>
      </div>
      <canvas
        ref={canvasRef}
        width={window.innerWidth}
        height={window.innerHeight - 40}
        className="flex-1 cursor-pointer"
        onClick={handleClick}
      />
    </div>
  );
}

export type { GraphNode } from "@/types/ipc";
