import { useRef, useCallback, useEffect } from 'react';
import type { LayoutNode, SplitNode, LeafNode, PanelKey } from '../types';
import { useBrainStore } from '../store';
import NoteListPanel from './NoteListPanel';
import EditorPanel from './EditorPanel';
import GraphPanel from './GraphPanel';
import ZoteroPanel from './ZoteroPanel';
import ChatPanel from './ChatPanel';
import './TileLayout.css';

const PANEL_LABELS: Record<PanelKey, string> = {
  notes: 'Notes',
  editor: 'Éditeur',
  graph: 'Graph',
  zotero: 'Zotero',
  chat: 'Chat',
};
const ALL_PANELS: PanelKey[] = ['notes', 'editor', 'graph', 'zotero', 'chat'];

function PanelContent({ panel }: { panel: PanelKey }) {
  if (panel === 'notes')  return <NoteListPanel />;
  if (panel === 'editor') return <EditorPanel />;
  if (panel === 'graph')  return <GraphPanel />;
  if (panel === 'zotero') return <ZoteroPanel />;
  if (panel === 'chat')   return <ChatPanel />;
  return null;
}

function getNodeAtPath(root: LayoutNode, path: number[]): LayoutNode | null {
  if (path.length === 0) return root;
  if (root.kind !== 'split') return null;
  const [head, ...rest] = path;
  return getNodeAtPath(head === 0 ? root.a : root.b, rest);
}

function setNodeAtPath(root: LayoutNode, path: number[], updater: (n: LayoutNode) => LayoutNode): LayoutNode {
  if (path.length === 0) return updater(root);
  if (root.kind !== 'split') return root;
  const [head, ...rest] = path;
  if (head === 0) return { ...root, a: setNodeAtPath(root.a, rest, updater) };
  return { ...root, b: setNodeAtPath(root.b, rest, updater) };
}

function LeafRenderer({ node, path }: { node: LeafNode; path: number[] }) {
  const { layout, setLayout } = useBrainStore();

  const changePanel = (panel: PanelKey) => {
    setLayout(setNodeAtPath(layout, path, () => ({ kind: 'leaf', panel })));
  };

  const splitH = () => {
    setLayout(setNodeAtPath(layout, path, (n) => ({
      kind: 'split', dir: 'h', ratio: 0.5,
      a: n, b: { kind: 'leaf', panel: 'editor' },
    })));
  };

  const splitV = () => {
    setLayout(setNodeAtPath(layout, path, (n) => ({
      kind: 'split', dir: 'v', ratio: 0.5,
      a: n, b: { kind: 'leaf', panel: 'editor' },
    })));
  };

  return (
    <div className="jb-tile-leaf">
      <div className="jb-tile-leaf-hdr">
        <div className="jb-tile-leaf-bar" />
        <select
          value={node.panel}
          onChange={e => changePanel(e.target.value as PanelKey)}
          className="jb-tile-panel-select"
        >
          {ALL_PANELS.map(p => (
            <option key={p} value={p}>{PANEL_LABELS[p]}</option>
          ))}
        </select>
        <div className="jb-tile-leaf-actions">
          <button onClick={splitH} title="Split horizontal">⊟</button>
          <button onClick={splitV} title="Split vertical">⊞</button>
        </div>
      </div>
      <div className="jb-tile-content">
        <PanelContent panel={node.panel} />
      </div>
    </div>
  );
}

function Divider({ dir, onDrag }: { dir: 'h' | 'v'; onDrag: (delta: number) => void }) {
  const dragging = useRef(false);
  const lastPos = useRef(0);
  // Keep ref current so onMove always calls the latest handler (avoids stale closure)
  const onDragRef = useRef(onDrag);
  useEffect(() => { onDragRef.current = onDrag; }, [onDrag]);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    lastPos.current = dir === 'h' ? e.clientX : e.clientY;

    const onMove = (ev: MouseEvent) => {
      if (!dragging.current) return;
      const pos = dir === 'h' ? ev.clientX : ev.clientY;
      onDragRef.current(pos - lastPos.current);
      lastPos.current = pos;
    };
    const onUp = () => {
      dragging.current = false;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }, [dir]);

  return (
    <div
      className={`jb-divider jb-divider--${dir}`}
      onMouseDown={onMouseDown}
    >
      <div className="jb-divider-grip">
        <span /><span /><span />
      </div>
    </div>
  );
}

function SplitRenderer({ node, path, containerRef }: {
  node: SplitNode;
  path: number[];
  containerRef: React.RefObject<HTMLDivElement | null>;
}) {
  const { setLayout } = useBrainStore();
  const nodeRef = useRef<HTMLDivElement>(null);

  const handleDrag = useCallback((delta: number) => {
    const el = nodeRef.current;
    if (!el) return;
    const size = node.dir === 'h' ? el.offsetWidth : el.offsetHeight;
    if (size === 0) return;
    // Functional update reads current layout from state — no stale closure
    setLayout(prevLayout => {
      const curNode = getNodeAtPath(prevLayout, path);
      const curRatio = curNode?.kind === 'split' ? curNode.ratio : node.ratio;
      const newRatio = Math.max(0.06, Math.min(0.94, curRatio + delta / size));
      return setNodeAtPath(prevLayout, path, (n) =>
        n.kind === 'split' ? { ...n, ratio: newRatio } : n
      );
    });
  }, [node.dir, node.ratio, path, setLayout]);

  const isH = node.dir === 'h';
  const pctA = `${node.ratio * 100}%`;
  const pctB = `${(1 - node.ratio) * 100}%`;

  return (
    <div
      ref={nodeRef}
      className="jb-split"
      style={{ flexDirection: isH ? 'row' : 'column' }}
    >
      <div style={{ [isH ? 'width' : 'height']: pctA, overflow: 'hidden', flexShrink: 0 }}>
        <NodeRenderer node={node.a} path={[...path, 0]} containerRef={nodeRef} />
      </div>
      <Divider dir={node.dir} onDrag={handleDrag} />
      <div style={{ [isH ? 'width' : 'height']: pctB, overflow: 'hidden', flex: 1 }}>
        <NodeRenderer node={node.b} path={[...path, 1]} containerRef={nodeRef} />
      </div>
    </div>
  );
}

function NodeRenderer({ node, path, containerRef }: {
  node: LayoutNode;
  path: number[];
  containerRef: React.RefObject<HTMLDivElement | null>;
}) {
  if (node.kind === 'leaf') return <LeafRenderer node={node} path={path} />;
  return <SplitRenderer node={node} path={path} containerRef={containerRef} />;
}

export default function TileLayout() {
  const { layout } = useBrainStore();
  const rootRef = useRef<HTMLDivElement>(null);
  return (
    <div ref={rootRef} style={{ width: '100%', height: '100%', overflow: 'hidden' }}>
      <NodeRenderer node={layout} path={[]} containerRef={rootRef} />
    </div>
  );
}
