import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { NoteMeta, Note, GraphData, LayoutNode, ZoteroItem, ChatMessage, Settings, ClaudeReply, GhostLink, PdfSearchResult, SemanticResult, NoteCluster, IncubatorNotif, VaultEntry } from './types';
import { makeLeaf, makeSplit } from './types';

const DEFAULT_LAYOUT: LayoutNode = makeSplit('h', 0.20,
  makeLeaf('notes'),
  makeSplit('h', 0.60,
    makeLeaf('editor'),
    makeSplit('v', 0.58,
      makeLeaf('graph'),
      makeLeaf('chat'),
    ),
  ),
);

interface BrainStore {
  // Vault
  notes: NoteMeta[];
  activeNote: Note | null;
  vaultPath: string;
  isLoading: boolean;

  // Vault registry (multi-vault)
  vaults: VaultEntry[];
  fetchVaults: () => Promise<void>;
  addVault: (name: string, path: string) => Promise<void>;
  renameVault: (path: string, name: string) => Promise<void>;
  removeVault: (path: string) => Promise<void>;
  switchVault: (path: string) => Promise<void>;

  // Graph
  graphData: GraphData | null;

  // Layout
  layout: LayoutNode;
  setLayout: (l: LayoutNode | ((prev: LayoutNode) => LayoutNode)) => void;

  // Modals
  showNewNote: boolean;
  setShowNewNote: (v: boolean) => void;
  showSettings: boolean;
  setShowSettings: (v: boolean) => void;

  // Settings
  settings: Settings;
  loadSettings: () => Promise<void>;
  saveSettings: (s: Settings) => Promise<void>;

  // Zotero
  zoteroItems: ZoteroItem[];
  zoteroLoading: boolean;
  zoteroError: string | null;
  fetchZotero: () => Promise<void>;
  importZoteroItem: (item: ZoteroItem) => Promise<void>;

  // Claude
  chatHistory: ChatMessage[];
  chatLoading: boolean;
  exploreResult: string | null;
  exploreLoading: boolean;
  claudeUsage: { input: number; output: number } | null;
  sendChat: (msg: string) => Promise<void>;
  clearChat: () => void;
  exploreLinks: () => Promise<void>;
  clearExplore: () => void;
  addLinkToNote: (path: string, linkTitle: string) => Promise<void>;

  // D3 Feynman questions
  feynmanResult: string | null;
  feynmanLoading: boolean;
  feynmanQuestion: () => Promise<void>;
  clearFeynman: () => void;

  // D4 Contradiction detection
  contradictionResult: string | null;
  contradictionLoading: boolean;
  detectContradictions: () => Promise<void>;
  clearContradictions: () => void;

  // D5 Multi-note Tangle
  selectedNotes: string[];
  tangleLoading: boolean;
  toggleNoteSelection: (path: string) => void;
  clearNoteSelection: () => void;
  generateTangle: () => Promise<void>;

  // D7 Ghost note generation
  ghostGenerating: string | null;
  generateGhostNote: (title: string, citedBy: string[]) => Promise<void>;

  // Ghost links
  ghostLinks: GhostLink[];
  fetchGhostLinks: () => Promise<void>;

  // PDF search
  pdfSearchResults: PdfSearchResult[];
  pdfSearchLoading: boolean;
  pdfIndexStatus: string | null;
  searchPdfFulltext: (query: string) => Promise<void>;
  buildPdfIndex: () => Promise<void>;

  // Semantic (Phase B)
  clusters: NoteCluster[];
  fetchClusters: () => Promise<void>;
  semanticResults: SemanticResult[];
  semanticLoading: boolean;
  searchSemantic: (query: string) => Promise<void>;
  suggestedTags: string[];
  suggestTagsLoading: boolean;
  suggestTagsMsg: string | null;
  fetchSuggestedTags: (path: string) => Promise<void>;
  applyTags: (path: string, tags: string[]) => Promise<void>;
  clearSuggestedTags: () => void;

  // C1 — Incubateur de connexions (Ollama background)
  incubatorNotifs: IncubatorNotif[];
  fetchIncubatorNotifs: () => Promise<void>;
  dismissIncubatorNotif: (index: number) => Promise<void>;
  acceptIncubatorConnection: (sourcePath: string, targetTitle: string, targetPath: string) => Promise<void>;

  // Daily note
  finalizeDailyNote: () => Promise<void>;
  yesterdayReminder: string | null;
  checkYesterdayDaily: () => Promise<void>;

  // Actions
  initVault: () => Promise<void>;
  loadVault: () => Promise<void>;
  openNote: (path: string) => Promise<void>;
  saveNote: (path: string, content: string) => Promise<void>;
  createNote: (type: string, title: string) => Promise<void>;
  deleteNote: (path: string) => Promise<void>;
  loadGraph: () => Promise<void>;
  updateNoteMeta: (path: string, interest?: number, status?: string) => Promise<void>;
}

export const useBrainStore = create<BrainStore>((set, get) => ({
  notes: [],
  activeNote: null,
  vaultPath: '',
  isLoading: false,
  vaults: [],

  fetchVaults: async () => {
    const vaults = await invoke<VaultEntry[]>('list_vaults');
    set({ vaults });
  },
  addVault: async (name, path) => {
    await invoke('add_vault', { name, path });
    const vaults = await invoke<VaultEntry[]>('list_vaults');
    set({ vaults });
  },
  renameVault: async (path, name) => {
    await invoke('rename_vault', { path, name });
    const vaults = await invoke<VaultEntry[]>('list_vaults');
    set({ vaults });
  },
  removeVault: async (path) => {
    await invoke('remove_vault', { path });
    const vaults = await invoke<VaultEntry[]>('list_vaults');
    set({ vaults });
  },
  switchVault: async (path) => {
    await invoke('switch_vault', { path });
    window.location.reload();
  },
  graphData: null,
  layout: DEFAULT_LAYOUT,
  showNewNote: false,
  showSettings: false,
  settings: {},
  zoteroItems: [],
  zoteroLoading: false,
  zoteroError: null,
  chatHistory: [],
  chatLoading: false,
  exploreResult: null,
  exploreLoading: false,
  claudeUsage: null,
  feynmanResult: null,
  feynmanLoading: false,
  contradictionResult: null,
  contradictionLoading: false,
  selectedNotes: [],
  tangleLoading: false,
  ghostGenerating: null,
  ghostLinks: [],
  pdfSearchResults: [],
  pdfSearchLoading: false,
  pdfIndexStatus: null,
  clusters: [],
  semanticResults: [],
  semanticLoading: false,
  suggestedTags: [],
  suggestTagsLoading: false,
  suggestTagsMsg: null,
  incubatorNotifs: [],
  yesterdayReminder: null,

  setLayout: (l) => set(state => ({
    layout: typeof l === 'function' ? l(state.layout) : l,
  })),
  setShowNewNote: (v) => set({ showNewNote: v }),
  setShowSettings: (v) => set({ showSettings: v }),

  loadSettings: async () => {
    try {
      const settings = await invoke<Settings>('get_settings');
      set({ settings });
    } catch (e) { console.error('get_settings failed:', e); }
  },

  saveSettings: async (settings) => {
    try {
      await invoke('save_settings', { settings });
      set({ settings });
    } catch (e) { console.error('save_settings failed:', e); }
  },

  fetchZotero: async () => {
    set({ zoteroLoading: true, zoteroError: null });
    try {
      const zoteroItems = await invoke<ZoteroItem[]>('fetch_zotero');
      set({ zoteroItems, zoteroLoading: false });
    } catch (e: unknown) {
      set({ zoteroLoading: false, zoteroError: String(e) });
    }
  },

  importZoteroItem: async (item) => {
    try {
      const path = await invoke<string>('import_zotero_item', { item });
      await get().loadVault();
      await get().openNote(path);
      // Refresh graph so TF-IDF links include the new note
      get().loadGraph().catch(() => {});
    } catch (e) { console.error('import_zotero_item failed:', e); }
  },

  sendChat: async (msg) => {
    const newHistory: ChatMessage[] = [
      ...get().chatHistory,
      { role: 'user', content: msg },
    ];
    set({ chatHistory: newHistory, chatLoading: true });
    try {
      const reply = await invoke<ClaudeReply>('claude_chat', { history: newHistory });
      const usage = reply.input_tokens != null
        ? { input: reply.input_tokens, output: reply.output_tokens ?? 0 }
        : null;
      set(s => ({
        chatHistory: [...s.chatHistory, { role: 'assistant', content: reply.text }],
        chatLoading: false,
        claudeUsage: usage ?? s.claudeUsage,
      }));
    } catch (e: unknown) {
      set(s => ({
        chatHistory: [...s.chatHistory, { role: 'assistant', content: `Erreur : ${e}` }],
        chatLoading: false,
      }));
    }
  },

  clearChat: () => set({ chatHistory: [], exploreResult: null }),

  exploreLinks: async () => {
    const note = get().activeNote;
    if (!note) return;
    set({ exploreLoading: true, exploreResult: null });
    try {
      const reply = await invoke<ClaudeReply>('claude_explore', {
        notePath: note.meta.path,
        noteTitle: note.meta.title,
        noteBody: note.content,
      });
      const usage = reply.input_tokens != null
        ? { input: reply.input_tokens, output: reply.output_tokens ?? 0 }
        : null;
      set({ exploreResult: reply.text, exploreLoading: false, ...(usage ? { claudeUsage: usage } : {}) });
    } catch (e: unknown) {
      set({ exploreResult: `Erreur : ${e}`, exploreLoading: false });
    }
  },

  clearExplore: () => set({ exploreResult: null }),

  feynmanQuestion: async () => {
    const note = get().activeNote;
    if (!note) return;
    set({ feynmanLoading: true, feynmanResult: null });
    try {
      const reply = await invoke<ClaudeReply>('claude_feynman', {
        noteTitle: note.meta.title,
        noteBody: note.content,
      });
      const usage = reply.input_tokens != null
        ? { input: reply.input_tokens, output: reply.output_tokens ?? 0 } : null;
      set({ feynmanResult: reply.text, feynmanLoading: false, ...(usage ? { claudeUsage: usage } : {}) });
    } catch (e) {
      set({ feynmanResult: `Erreur : ${e}`, feynmanLoading: false });
    }
  },

  clearFeynman: () => set({ feynmanResult: null }),

  detectContradictions: async () => {
    const note = get().activeNote;
    if (!note) return;
    set({ contradictionLoading: true, contradictionResult: null });
    try {
      const reply = await invoke<ClaudeReply>('claude_contradictions', { notePath: note.meta.path });
      const usage = reply.input_tokens != null
        ? { input: reply.input_tokens, output: reply.output_tokens ?? 0 } : null;
      set({ contradictionResult: reply.text, contradictionLoading: false, ...(usage ? { claudeUsage: usage } : {}) });
    } catch (e) {
      set({ contradictionResult: `Erreur : ${e}`, contradictionLoading: false });
    }
  },

  clearContradictions: () => set({ contradictionResult: null }),

  toggleNoteSelection: (path) => set(s => ({
    selectedNotes: s.selectedNotes.includes(path)
      ? s.selectedNotes.filter(p => p !== path)
      : [...s.selectedNotes, path],
  })),

  clearNoteSelection: () => set({ selectedNotes: [] }),

  generateTangle: async () => {
    const paths = get().selectedNotes;
    if (paths.length < 2) return;
    set({ tangleLoading: true });
    try {
      const path = await invoke<string>('claude_tangle', { paths });
      set({ tangleLoading: false, selectedNotes: [] });
      await get().loadVault();
      await get().openNote(path);
      get().loadGraph().catch(() => {});
    } catch (e) {
      console.error('claude_tangle failed:', e);
      set({ tangleLoading: false });
    }
  },

  generateGhostNote: async (title, citedBy) => {
    set({ ghostGenerating: title });
    try {
      const path = await invoke<string>('claude_generate_ghost', { title, citedBy });
      set({ ghostGenerating: null });
      await get().loadVault();
      await get().openNote(path);
      await get().fetchGhostLinks();
      get().loadGraph().catch(() => {});
    } catch (e) {
      console.error('claude_generate_ghost failed:', e);
      set({ ghostGenerating: null });
    }
  },

  fetchGhostLinks: async () => {
    try {
      const ghostLinks = await invoke<GhostLink[]>('get_ghost_links');
      set({ ghostLinks });
    } catch (e) { console.error('get_ghost_links failed:', e); }
  },

  searchPdfFulltext: async (query) => {
    if (!query.trim()) { set({ pdfSearchResults: [] }); return; }
    set({ pdfSearchLoading: true });
    try {
      const pdfSearchResults = await invoke<PdfSearchResult[]>('search_pdf_fulltext', { query });
      set({ pdfSearchResults, pdfSearchLoading: false });
    } catch (e) {
      console.error('search_pdf_fulltext failed:', e);
      set({ pdfSearchLoading: false });
    }
  },

  buildPdfIndex: async () => {
    set({ pdfIndexStatus: null });
    try {
      const count = await invoke<number>('build_pdf_index');
      set({ pdfIndexStatus: count > 0 ? `✓ ${count} PDF(s) indexé(s)` : '✓ Indexé (0 nouveaux PDFs)' });
    } catch (e) {
      set({ pdfIndexStatus: `Erreur : ${e}` });
    }
  },

  fetchClusters: async () => {
    try {
      const clusters = await invoke<NoteCluster[]>('get_clusters');
      set({ clusters });
    } catch { /* Ollama not available — clusters stay empty */ }
  },

  searchSemantic: async (query) => {
    if (!query.trim()) { set({ semanticResults: [] }); return; }
    set({ semanticLoading: true });
    try {
      const semanticResults = await invoke<SemanticResult[]>('search_semantic', { query });
      set({ semanticResults, semanticLoading: false });
    } catch (e) {
      console.error('search_semantic failed:', e);
      set({ semanticLoading: false });
    }
  },

  fetchSuggestedTags: async (path) => {
    set({ suggestTagsLoading: true, suggestedTags: [], suggestTagsMsg: null });
    try {
      const suggestedTags = await invoke<string[]>('suggest_note_tags', { path });
      if (suggestedTags.length === 0) {
        set({ suggestedTags: [], suggestTagsLoading: false, suggestTagsMsg: 'no_model' });
      } else {
        set({ suggestedTags, suggestTagsLoading: false, suggestTagsMsg: null });
      }
    } catch {
      set({ suggestTagsLoading: false, suggestTagsMsg: 'error' });
    }
  },

  applyTags: async (path, tags) => {
    try {
      await invoke('apply_note_tags', { path, tags });
      await get().loadVault();
      await get().openNote(path);
    } catch (e) { console.error('apply_note_tags failed:', e); }
  },

  clearSuggestedTags: () => set({ suggestedTags: [], suggestTagsLoading: false, suggestTagsMsg: null }),

  fetchIncubatorNotifs: async () => {
    try {
      const incubatorNotifs = await invoke<IncubatorNotif[]>('get_incubator_notifs');
      set({ incubatorNotifs });
    } catch { /* silent — incubator is background only */ }
  },

  dismissIncubatorNotif: async (index) => {
    try {
      await invoke('dismiss_incubator_notif', { index });
      set(s => ({ incubatorNotifs: s.incubatorNotifs.filter((_, i) => i !== index) }));
    } catch (e) { console.error('dismiss_incubator_notif failed:', e); }
  },

  acceptIncubatorConnection: async (sourcePath, targetTitle, targetPath) => {
    try {
      await invoke('accept_incubator_connection', { sourcePath, targetTitle });
      // Reload the source note so the new link appears in the editor
      const currentActive = get().activeNote;
      if (currentActive?.meta.path === sourcePath) await get().openNote(sourcePath);
      await get().loadGraph();
      set(s => ({
        incubatorNotifs: s.incubatorNotifs.filter(
          n => !(n.source_path === sourcePath && n.target_title === targetTitle)
        ),
      }));
    } catch (e) { console.error('accept_incubator_connection failed:', e); }
  },

  finalizeDailyNote: async () => {
    try { await invoke('finalize_daily_note'); } catch { /* silent */ }
  },

  checkYesterdayDaily: async () => {
    try {
      const date = await invoke<string | null>('check_yesterday_daily');
      set({ yesterdayReminder: date });
    } catch { /* silent */ }
  },

  addLinkToNote: async (path, linkTitle) => {
    try {
      await invoke('add_link_to_note', { path, linkTitle });
      // Reload the note to reflect the new link in the editor
      await get().openNote(path);
    } catch (e) { console.error('add_link_to_note failed:', e); }
  },

  initVault: async () => {
    try {
      const vaultPath = await invoke<string>('init_vault');
      set({ vaultPath });
      await Promise.all([get().loadVault(), get().loadSettings(), get().fetchVaults()]);
      // Background tasks — don't block startup
      invoke<string>('detect_zotero_db').then(dbPath => {
        if (dbPath) get().fetchZotero().catch(() => {});
      }).catch(() => {});
      // Clusters are computed from the Ollama cache (sync, fast)
      get().fetchClusters().catch(() => {});
      // Morning reminder: check if yesterday's daily note has an empty "Ma journée"
      get().checkYesterdayDaily().catch(() => {});
    } catch (e) {
      console.error('init_vault failed:', e);
    }
  },

  loadVault: async () => {
    set({ isLoading: true });
    try {
      const notes = await invoke<NoteMeta[]>('get_vault_notes');
      set({ notes, isLoading: false });
    } catch (e) {
      console.error('get_vault_notes failed:', e);
      set({ isLoading: false });
    }
  },

  openNote: async (path) => {
    try {
      const note = await invoke<Note>('read_note', { path });
      set({ activeNote: note });
    } catch (e) {
      console.error('read_note failed:', e);
    }
  },

  saveNote: async (path, content) => {
    try {
      await invoke('save_note', { path, content });
      // Refresh note meta in list without full reload
      const notes = get().notes.map(n =>
        n.path === path ? { ...n } : n
      );
      set({ notes });
      // Reload graph so new wikilinks appear without needing to switch panels
      get().loadGraph().catch(() => {});
    } catch (e) {
      console.error('save_note failed:', e);
    }
  },

  createNote: async (noteType, title) => {
    try {
      const path = await invoke<string>('create_note', { noteType, title });
      await get().loadVault();
      await get().openNote(path);
    } catch (e) {
      console.error('create_note failed:', e);
    }
  },

  deleteNote: async (path) => {
    try {
      await invoke('delete_note', { path });
      const activeNote = get().activeNote;
      if (activeNote?.meta.path === path) set({ activeNote: null });
      await get().loadVault();
    } catch (e) {
      console.error('delete_note failed:', e);
    }
  },

  loadGraph: async () => {
    try {
      const graphData = await invoke<GraphData>('get_graph_data');
      set({ graphData });
    } catch (e) {
      console.error('get_graph_data failed:', e);
    }
  },

  updateNoteMeta: async (path, interest, status) => {
    try {
      await invoke('update_note_meta', { path, interest, status });
      // Instant local update — no round-trip needed
      set(state => ({
        notes: state.notes.map(n =>
          n.path !== path ? n : {
            ...n,
            ...(interest !== undefined ? { interest } : {}),
            ...(status !== undefined ? { status } : {}),
          }
        ),
        activeNote: state.activeNote?.meta.path === path
          ? {
              ...state.activeNote,
              meta: {
                ...state.activeNote.meta,
                ...(interest !== undefined ? { interest } : {}),
                ...(status !== undefined ? { status } : {}),
              },
            }
          : state.activeNote,
      }));
    } catch (e) {
      console.error('update_note_meta failed:', e);
    }
  },
}));
