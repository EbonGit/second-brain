import { useEffect, useRef, useCallback, useState, useMemo } from 'react';
import * as d3 from 'd3';
import { invoke } from '@tauri-apps/api/core';
import { useBrainStore } from '../store';
import { NOTE_TYPE_COLOR, type GraphNode, type GhostLink } from '../types';
import IncubatorPanel from './IncubatorPanel';
import './GraphPanel.css';
import './IncubatorPanel.css';

const CLUSTER_COLORS = [
  'rgba(91,123,212,0.22)',
  'rgba(186,85,140,0.22)',
  'rgba(80,185,140,0.22)',
  'rgba(212,165,70,0.22)',
  'rgba(130,91,212,0.22)',
  'rgba(70,190,210,0.22)',
];

export default function GraphPanel() {
  const { graphData, loadGraph, openNote, ghostLinks, fetchGhostLinks, clusters,
          incubatorNotifs, fetchIncubatorNotifs } = useBrainStore();
  const svgRef        = useRef<SVGSVGElement>(null);
  const containerRef  = useRef<HTMLDivElement>(null);
  const rafRef        = useRef<number>(0);
  const rawEdgesRef   = useRef<any[]>([]);
  const simNodesRef   = useRef<any[]>([]);
  const zoomRef       = useRef<d3.ZoomBehavior<SVGSVGElement, unknown> | null>(null);
  // Ref so D3 click handlers always read the latest focusId without stale closure
  const focusIdRef    = useRef<string | null>(null);

  const [keywords, setKeywords]       = useState<string[]>([]);
  const [keywordsOpen, setKeywordsOpen] = useState(false);
  const [focusId, setFocusId]         = useState<string | null>(null);
  const [incubatorOpen, setIncubatorOpen] = useState(false);

  useEffect(() => {
    loadGraph();
    fetchGhostLinks();
    invoke<string[]>('get_semantic_keywords').then(setKeywords).catch(() => {});
    // Poll incubator notifs every 15s (background Ollama may produce them any time)
    fetchIncubatorNotifs();
    const poll = setInterval(() => fetchIncubatorNotifs(), 15_000);
    return () => clearInterval(poll);
  }, []);

  // Memoize so buildGraph only recreates when cluster data actually changes, not every render.
  // Without this, every setFocusId → re-render → new Map → new buildGraph → ResizeObserver
  // reinstalls → obs.observe fires → graph rebuilds and resets focus immediately.
  const clusterMap = useMemo(
    () => new Map(clusters.map(c => [c.path, c.cluster])),
    [clusters]
  );

  const buildGraph = useCallback(() => {
    if (!graphData || !svgRef.current || !containerRef.current) return;

    const { width, height } = containerRef.current.getBoundingClientRect();
    if (width === 0 || height === 0) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll('*').remove();
    // Reset focus when graph data changes
    setFocusId(null);

    const g = svg.append('g');

    const zoom = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.15, 4])
      .on('zoom', (e) => g.attr('transform', e.transform));
    svg.call(zoom);
    zoomRef.current = zoom;

    const nodes: (GraphNode & { isGhost?: boolean; cited_by?: string[] })[] =
      graphData.nodes.map(n => ({ ...n }));

    ghostLinks.forEach((gl: GhostLink) => {
      nodes.push({
        id: `ghost:${gl.title}`,
        title: gl.title,
        note_type: 'note',
        tags: [],
        weight: 0.5,
        isGhost: true,
        cited_by: gl.cited_by,
      });
    });

    const rawEdges: any[] = graphData.edges.map(e => ({ ...e }));
    ghostLinks.forEach((gl: GhostLink) => {
      gl.cited_by.forEach(sourceTitle => {
        const src = nodes.find(n => !n.isGhost && n.title === sourceTitle);
        if (src) rawEdges.push({ source: src.id, target: `ghost:${gl.title}`, edge_type: 'ghost', weight: 0.3 });
      });
    });
    rawEdgesRef.current = rawEdges;
    simNodesRef.current = nodes as any[];

    const getSameCluster = (d: any) => {
      const sid = typeof d.source === 'object' ? d.source.id : d.source;
      const tid = typeof d.target === 'object' ? d.target.id : d.target;
      const sc = clusterMap.get(sid);
      return sc !== undefined && sc === clusterMap.get(tid);
    };

    const sim = d3.forceSimulation(nodes as d3.SimulationNodeDatum[])
      .force('link', d3.forceLink(rawEdges)
        .id((d: any) => d.id)
        .distance((d: any) => {
          const base = d.edge_type === 'explicit' ? 70 : d.edge_type === 'tag' ? 120 : 160;
          return getSameCluster(d) ? base * 0.55 : base;
        })
        .strength((d: any) => {
          const base = d.edge_type === 'explicit' ? 0.7 : d.edge_type === 'tag' ? 0.2 : 0.08;
          return getSameCluster(d) ? Math.min(0.95, base * 2.5) : base;
        })
      )
      .force('charge', d3.forceManyBody().strength(-180))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide((d: any) => 8 + d.weight * 3));

    const link = g.append('g').selectAll('line')
      .data(rawEdges)
      .enter().append('line')
      .attr('class', (d: any) => `jb-graph-edge jb-graph-edge--${d.edge_type}`);

    const node = g.append('g').selectAll<SVGGElement, GraphNode>('g')
      .data(nodes)
      .enter().append('g')
      .attr('class', 'jb-graph-node')
      .call(
        d3.drag<SVGGElement, GraphNode>()
          .on('start', (e, d: any) => { if (!e.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; })
          .on('drag',  (e, d: any) => { d.fx = e.x; d.fy = e.y; })
          .on('end',   (e, d: any) => { if (!e.active) sim.alphaTarget(0); d.fx = null; d.fy = null; })
      )
      .on('click', (_, d) => {
        // Use ref (not state) to read current focus without stale closure
        if (focusIdRef.current === d.id) {
          // Second click on already-focused node → open the note
          openNote(d.id);
          setFocusId(null);
        } else {
          setFocusId(d.id);
        }
      });

    // Cluster halo drawn first (behind the node shape)
    node.each(function(d: any) {
      if (d.isGhost) return;
      const cl = clusterMap.get(d.id);
      if (cl === undefined) return;
      const r = 5 + d.weight * 2.5;
      d3.select(this).append('circle')
        .attr('r', r + 6)
        .attr('fill', CLUSTER_COLORS[cl % CLUSTER_COLORS.length])
        .attr('stroke', 'none')
        .attr('class', 'jb-graph-cluster-halo');
    });

    // Node shapes
    node.each(function(d: any) {
      const el = d3.select(this);
      const r = 5 + d.weight * 2.5;
      if (d.isGhost) {
        el.append('circle').attr('r', 5)
          .attr('fill', 'rgba(120,120,120,0.08)')
          .attr('stroke', '#666').attr('stroke-width', 1.2).attr('stroke-dasharray', '3,2');
      } else if (d.note_type === 'tangle') {
        el.append('polygon')
          .attr('points', starPoints(0, 0, r, r * 0.45, 5))
          .attr('fill', NOTE_TYPE_COLOR[d.note_type as keyof typeof NOTE_TYPE_COLOR])
          .attr('opacity', 0.9).attr('stroke', 'rgba(0,0,0,0.3)').attr('stroke-width', 1);
      } else {
        el.append('circle').attr('r', r)
          .attr('fill', NOTE_TYPE_COLOR[d.note_type as keyof typeof NOTE_TYPE_COLOR] || '#666')
          .attr('opacity', 0.85).attr('stroke', 'rgba(0,0,0,0.25)').attr('stroke-width', 1);
      }
    });

    node.append('text')
      .text(d => d.title.length > 18 ? d.title.slice(0, 17) + '…' : d.title)
      .attr('class', 'jb-graph-label')
      .attr('x', (d) => 7 + d.weight * 2.5)
      .attr('y', 4);

    sim.on('tick', () => {
      link
        .attr('x1', (d: any) => d.source.x ?? 0).attr('y1', (d: any) => d.source.y ?? 0)
        .attr('x2', (d: any) => d.target.x ?? 0).attr('y2', (d: any) => d.target.y ?? 0);
      node.attr('transform', (d: any) => `translate(${d.x ?? 0},${d.y ?? 0})`);
    });

    return () => { sim.stop(); };
  }, [graphData, openNote, ghostLinks, clusterMap]);

  // Direct rebuild when data changes (doesn't rely on a resize event firing)
  useEffect(() => {
    if (!graphData) return;
    const r = requestAnimationFrame(() => { buildGraph(); });
    return () => cancelAnimationFrame(r);
  }, [buildGraph]);

  // Additional rebuild on container resize (handles panel splits being dragged)
  useEffect(() => {
    if (!containerRef.current) return;
    const obs = new ResizeObserver(entries => {
      const { width, height } = entries[0].contentRect;
      if (width > 0 && height > 0) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = requestAnimationFrame(() => { buildGraph(); });
      }
    });
    obs.observe(containerRef.current);
    return () => {
      obs.disconnect();
      cancelAnimationFrame(rafRef.current);
    };
  }, [buildGraph]);

  // Sync focus ref, apply opacity dimming, and zoom to fit the 2-hop neighborhood
  useEffect(() => {
    focusIdRef.current = focusId;

    const svg = d3.select(svgRef.current);
    const zoom = zoomRef.current;
    const container = containerRef.current;

    if (!focusId) {
      // Restore full opacity
      svg.selectAll('.jb-graph-node').attr('opacity', 1);
      svg.selectAll('.jb-graph-edge').attr('opacity', 1);
      // Zoom back to fit all nodes
      if (zoom && container && simNodesRef.current.length > 0) {
        const allNodes = simNodesRef.current;
        const xs = allNodes.map((n: any) => n.x ?? 0);
        const ys = allNodes.map((n: any) => n.y ?? 0);
        const { width, height } = container.getBoundingClientRect();
        const pad = 50;
        const bw = Math.max(1, Math.max(...xs) - Math.min(...xs));
        const bh = Math.max(1, Math.max(...ys) - Math.min(...ys));
        const sc = Math.min((width - pad * 2) / bw, (height - pad * 2) / bh, 1.5);
        const cx = (Math.min(...xs) + Math.max(...xs)) / 2;
        const cy = (Math.min(...ys) + Math.max(...ys)) / 2;
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        svg.transition().duration(450).call(zoom.transform as any,
          d3.zoomIdentity.translate(width / 2 - sc * cx, height / 2 - sc * cy).scale(sc)
        );
      }
      return;
    }

    // Collect 2-hop neighborhood
    const neighbors = new Set<string>([focusId]);
    const edges = rawEdgesRef.current;
    edges.forEach((e: any) => {
      const s = typeof e.source === 'object' ? e.source.id : e.source;
      const t = typeof e.target === 'object' ? e.target.id : e.target;
      if (s === focusId) neighbors.add(t);
      if (t === focusId) neighbors.add(s);
    });
    edges.forEach((e: any) => {
      const s = typeof e.source === 'object' ? e.source.id : e.source;
      const t = typeof e.target === 'object' ? e.target.id : e.target;
      if (neighbors.has(s)) neighbors.add(t);
      if (neighbors.has(t)) neighbors.add(s);
    });

    // Dim non-neighbors to 30% (70% transparent)
    svg.selectAll<SVGGElement, any>('.jb-graph-node')
      .attr('opacity', d => neighbors.has(d.id) ? 1 : 0.3);
    svg.selectAll<SVGLineElement, any>('.jb-graph-edge')
      .attr('opacity', (d: any) => {
        const s = typeof d.source === 'object' ? d.source.id : d.source;
        const t = typeof d.target === 'object' ? d.target.id : d.target;
        return neighbors.has(s) && neighbors.has(t) ? 0.8 : 0.08;
      });

    // Zoom to fit neighborhood bounding box
    if (!zoom || !container) return;
    const neighborNodes = simNodesRef.current.filter((n: any) => neighbors.has(n.id));
    if (neighborNodes.length === 0) return;
    const xs = neighborNodes.map((n: any) => n.x ?? 0);
    const ys = neighborNodes.map((n: any) => n.y ?? 0);
    const { width, height } = container.getBoundingClientRect();
    const pad = 80;
    const bw = Math.max(1, Math.max(...xs) - Math.min(...xs));
    const bh = Math.max(1, Math.max(...ys) - Math.min(...ys));
    const sc = Math.min((width - pad * 2) / bw, (height - pad * 2) / bh, 3);
    const cx = (Math.min(...xs) + Math.max(...xs)) / 2;
    const cy = (Math.min(...ys) + Math.max(...ys)) / 2;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    svg.transition().duration(450).call(zoom.transform as any,
      d3.zoomIdentity.translate(width / 2 - sc * cx, height / 2 - sc * cy).scale(sc)
    );
  }, [focusId]);

  return (
    <div ref={containerRef} className="jb-graph-panel">
      {(!graphData || graphData.nodes.length === 0) ? (
        <div className="jb-graph-empty">
          <span>◈</span>
          <p>Crée des notes pour voir le graph</p>
        </div>
      ) : (
        <>
          <svg ref={svgRef} className="jb-graph-svg" />

          {focusId && (
            <button
              className="jb-graph-reset-focus"
              onClick={() => setFocusId(null)}
              title="Afficher tout le graphe"
            >
              ✕ Reset focus
            </button>
          )}

          {/* C1 — Incubateur badge */}
          {incubatorNotifs.length > 0 && !incubatorOpen && (
            <button
              className="jb-incubator-badge"
              onClick={() => setIncubatorOpen(true)}
              title="Connexions suggérées par Ollama"
            >
              <span className="jb-incubator-badge-dot" />
              {incubatorNotifs.length} connexion{incubatorNotifs.length > 1 ? 's' : ''} suggérée{incubatorNotifs.length > 1 ? 's' : ''}
            </button>
          )}

          {/* C1 — Incubateur panel */}
          {incubatorOpen && (
            <IncubatorPanel onClose={() => setIncubatorOpen(false)} />
          )}

          <div className="jb-graph-legend">
            {(['paper','idea','experiment','tangle','daily'] as const).map(t => (
              <div key={t} className="jb-graph-legend-item">
                <span className="jb-graph-legend-dot" style={{ background: NOTE_TYPE_COLOR[t] }} />
                <span>{t}</span>
              </div>
            ))}
            <div className="jb-graph-legend-item">
              <svg width="20" height="8"><line x1="0" y1="4" x2="20" y2="4" stroke="#555" strokeWidth="1.5" /></svg>
              <span>explicite</span>
            </div>
            <div className="jb-graph-legend-item">
              <svg width="20" height="8"><line x1="0" y1="4" x2="20" y2="4" stroke="#555" strokeWidth="1" strokeDasharray="3,2" /></svg>
              <span>tag</span>
            </div>
          </div>

          <div className="jb-graph-stats">
            {graphData.nodes.length} nœuds · {graphData.edges.length} liens
            {focusId && <span className="jb-graph-focus-hint"> · reclic = ouvrir</span>}
            <button
              className="jb-graph-refresh-btn"
              onClick={loadGraph}
              title="Rafraîchir le graphe"
            >↺</button>
          </div>

          {keywords.length > 0 && (
            <div className="jb-graph-keywords">
              <button
                className="jb-graph-keywords-toggle"
                onClick={() => setKeywordsOpen(o => !o)}
                title="Concepts centraux (Ollama)"
              >
                {keywordsOpen ? '▾' : '▸'} concepts centraux
              </button>
              {keywordsOpen && keywords.map(k => (
                <span key={k} className="jb-graph-keyword-pill">{k}</span>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

function starPoints(cx: number, cy: number, outerR: number, innerR: number, points: number): string {
  const step = Math.PI / points;
  return Array.from({ length: points * 2 }, (_, i) => {
    const angle = i * step - Math.PI / 2;
    const r = i % 2 === 0 ? outerR : innerR;
    return `${cx + r * Math.cos(angle)},${cy + r * Math.sin(angle)}`;
  }).join(' ');
}
