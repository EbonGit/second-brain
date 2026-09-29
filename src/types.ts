export type NoteType = 'paper' | 'idea' | 'experiment' | 'tangle' | 'daily' | 'note';

export interface NoteMeta {
  id: string;
  title: string;
  note_type: NoteType;
  path: string;
  tags: string[];
  links: string[];
  status?: string;
  interest?: number;
  created: string;
  modified: string;
  maturity_score?: number;
}

export interface Note {
  meta: NoteMeta;
  content: string;
  raw: string;
}

export interface GraphNode {
  id: string;
  title: string;
  note_type: NoteType;
  tags: string[];
  weight: number;
  x?: number;
  y?: number;
  vx?: number;
  vy?: number;
  fx?: number | null;
  fy?: number | null;
}

export interface GraphEdge {
  source: string | GraphNode;
  target: string | GraphNode;
  edge_type: 'explicit' | 'tag' | 'tfidf';
  weight: number;
}

export interface ZoteroItem {
  key: string;
  title: string;
  authors: string[];
  year?: string;
  doi?: string;
  abstract_note?: string;
  item_type: string;
  tags: string[];
  url?: string;
}

export interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
}

export interface ClaudeReply {
  text: string;
  input_tokens?: number;
  output_tokens?: number;
}

export interface ZoteroAnnotation {
  text: string;
  comment?: string;
  color?: string;
  page_label?: string;
}

export interface GhostLink {
  title: string;
  cited_by: string[];
}

export interface PdfSearchResult {
  zotero_key: string;
  title: string;
  excerpt: string;
}

export interface SemanticResult {
  path: string;
  title: string;
  score: number;
}

export interface NoteCluster {
  path: string;
  cluster: number;
}

export interface IncubatorNotif {
  source_path: string;
  source_title: string;
  target_path: string;
  target_title: string;
  score: number;
}

export interface VaultEntry {
  name: string;
  path: string;
}

export interface Settings {
  use_claude_cli?: boolean;      // true (default) = Claude Code CLI, false = API key below
  anthropic_api_key?: string;
  zotero_db_path?: string;       // custom path to zotero.sqlite (auto-detected if absent)
  zotero_api_key?: string;       // only needed if no local DB
  zotero_user_id?: string;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export const NOTE_TYPE_COLOR: Record<NoteType, string> = {
  paper:      'var(--jb-paper)',
  idea:       'var(--jb-idea)',
  experiment: 'var(--jb-experiment)',
  tangle:     'var(--jb-tangle)',
  daily:      'var(--jb-daily)',
  note:       'var(--jb-text-dim)',
};

export const NOTE_TYPE_LABEL: Record<NoteType, string> = {
  paper:      'Papers',
  idea:       'Idées',
  experiment: 'Expériences',
  tangle:     'Tangle Nodes',
  daily:      'Daily Notes',
  note:       'Notes',
};

export const NOTE_TYPES: NoteType[] = ['paper', 'idea', 'experiment', 'tangle', 'daily'];

// Panel system
export type PanelKey = 'notes' | 'editor' | 'graph' | 'zotero' | 'chat';

export type LeafNode = { kind: 'leaf'; panel: PanelKey };
export type SplitNode = {
  kind: 'split';
  dir: 'h' | 'v';
  ratio: number;
  a: LayoutNode;
  b: LayoutNode;
};
export type LayoutNode = LeafNode | SplitNode;

export function makeLeaf(panel: PanelKey): LeafNode {
  return { kind: 'leaf', panel };
}
export function makeSplit(dir: 'h' | 'v', ratio: number, a: LayoutNode, b: LayoutNode): SplitNode {
  return { kind: 'split', dir, ratio, a, b };
}
