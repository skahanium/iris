export interface GraphLayoutNode {
  id: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  radius: number;
}

export interface GraphLayoutEdge {
  source: number;
  target: number;
}

export interface GraphLayoutRequest {
  requestId: number;
  nodes: GraphLayoutNode[];
  edges: GraphLayoutEdge[];
  width: number;
  height: number;
  iterations: number;
}

export interface GraphLayoutResponse {
  requestId: number;
  nodes: GraphLayoutNode[];
}

function forceSimulate(
  nodes: GraphLayoutNode[],
  edges: GraphLayoutEdge[],
  nodeById: Map<number, GraphLayoutNode>,
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

  for (let iter = 0; iter < iterations; iter += 1) {
    for (let i = 0; i < nodes.length; i += 1) {
      for (let j = i + 1; j < nodes.length; j += 1) {
        const a = nodes[i]!;
        const b = nodes[j]!;
        const dx = b.x - a.x;
        const dy = b.y - a.y;
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

    for (const edge of edges) {
      const src = nodeById.get(edge.source);
      const tgt = nodeById.get(edge.target);
      if (!src || !tgt) continue;
      const ex = tgt.x - src.x;
      const ey = tgt.y - src.y;
      const edist = Math.sqrt(ex * ex + ey * ey) || 1;
      const eforce = edist * kAttract;
      const efx = (ex / edist) * eforce;
      const efy = (ey / edist) * eforce;
      src.vx += efx;
      src.vy += efy;
      tgt.vx -= efx;
      tgt.vy -= efy;
    }

    for (const node of nodes) {
      node.vx += (cx - node.x) * 0.001;
      node.vy += (cy - node.y) * 0.001;
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

self.onmessage = (event: MessageEvent<GraphLayoutRequest>) => {
  const { requestId, width, height, iterations } = event.data;
  const nodes = event.data.nodes.map((node) => ({ ...node }));
  const edges = event.data.edges.map((edge) => ({ ...edge }));
  const nodeById = new Map(nodes.map((node) => [node.id, node]));

  forceSimulate(nodes, edges, nodeById, width, height, iterations);

  self.postMessage({ requestId, nodes } satisfies GraphLayoutResponse);
};
