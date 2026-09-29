import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useBrainStore } from '../store';
import type { ZoteroItem, ZoteroAnnotation } from '../types';
import './ZoteroPanel.css';

async function persistToLinkedNote(
  linkedPath: string,
  sectionTitle: string,
  content: string,
  activePath: string | undefined,
  openNote: (p: string) => Promise<void>,
) {
  try {
    await invoke('append_note_section', { path: linkedPath, sectionTitle, content });
    if (activePath === linkedPath) await openNote(linkedPath);
  } catch { /* silent — showing inline is enough */ }
}

export default function ZoteroPanel() {
  const { zoteroItems, zoteroLoading, zoteroError, fetchZotero, importZoteroItem } = useBrainStore();

  return (
    <div className="jb-zotero">
      <div className="jb-zotero-toolbar">
        <span className="jb-zotero-title">Zotero Library</span>
        <button
          className="jb-zotero-refresh"
          onClick={fetchZotero}
          disabled={zoteroLoading}
          title="Synchroniser avec Zotero"
        >
          {zoteroLoading ? '⟳' : '↺'}
        </button>
      </div>

      {/* Only show error when no items loaded — if we have items, error is a stale refresh warning */}
      {zoteroError && zoteroItems.length === 0 && (
        <div className="jb-zotero-error">{zoteroError}</div>
      )}

      {zoteroLoading && (
        <div className="jb-zotero-hint">Chargement de la bibliothèque…</div>
      )}

      {zoteroItems.length === 0 && !zoteroLoading && !zoteroError && (
        <div className="jb-zotero-hint">
          Bibliothèque vide ou Zotero non trouvé.<br />
          <span style={{ opacity: 0.7 }}>Clique ↺ pour relancer. La base locale est auto-détectée — aucun compte requis.</span>
        </div>
      )}

      {zoteroItems.length > 0 && !zoteroLoading && (
        <div className="jb-zotero-count">{zoteroItems.length} références chargées</div>
      )}

      <div className="jb-zotero-list">
        {zoteroItems.map(item => (
          <ZoteroItemRow key={item.key} item={item} onImport={importZoteroItem} />
        ))}
      </div>
    </div>
  );
}

const ANNOT_COLOR: Record<string, string> = {
  '#ffd400': '#b8950a',
  '#ff6666': '#c0392b',
  '#5fb236': '#27ae60',
  '#2ea8e5': '#2980b9',
  '#a28ae5': '#8e44ad',
};

function ZoteroItemRow({ item, onImport }: { item: ZoteroItem; onImport: (i: ZoteroItem) => void }) {
  const { notes, activeNote, openNote } = useBrainStore();
  const linkedNote = notes.find(n => n.title.toLowerCase() === item.title.toLowerCase());

  const authorStr = item.authors.length > 0
    ? item.authors.slice(0, 2).join(', ') + (item.authors.length > 2 ? ' et al.' : '')
    : '';
  const [annotations, setAnnotations] = useState<ZoteroAnnotation[] | null>(null);
  const [annotLoading, setAnnotLoading] = useState(false);
  const [summary, setSummary] = useState<string | null>(null);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [refs, setRefs] = useState<string[] | null>(null);
  const [refsLoading, setRefsLoading] = useState(false);
  const [refsError, setRefsError] = useState<string | null>(null);

  const loadAnnotations = async () => {
    if (annotations !== null) { setAnnotations(null); return; }
    setAnnotLoading(true);
    try {
      const result = await invoke<ZoteroAnnotation[]>('fetch_zotero_annotations', { zoteroKey: item.key });
      setAnnotations(result);
    } catch { setAnnotations([]); }
    setAnnotLoading(false);
  };

  const loadSummary = async () => {
    if (summary !== null) { setSummary(null); setSummaryError(null); return; }
    setSummaryLoading(true); setSummaryError(null);
    try {
      const reply = await invoke<{ text: string }>('claude_pdf_summary', { zoteroKey: item.key });
      setSummary(reply.text);
      if (linkedNote) {
        await persistToLinkedNote(
          linkedNote.path, 'Résumé (Claude)', reply.text,
          activeNote?.meta.path, openNote,
        );
      }
    } catch (e) { setSummaryError(String(e)); }
    setSummaryLoading(false);
  };

  const loadRefs = async () => {
    if (refs !== null) { setRefs(null); setRefsError(null); return; }
    setRefsLoading(true); setRefsError(null);
    try {
      const result = await invoke<string[]>('extract_pdf_refs', { zoteroKey: item.key });
      setRefs(result);
      if (linkedNote && result.length > 0) {
        const refsMarkdown = result.map((r, i) => `${i + 1}. ${r}`).join('\n');
        await persistToLinkedNote(
          linkedNote.path, 'Références citées', refsMarkdown,
          activeNote?.meta.path, openNote,
        );
      }
    } catch (e) { setRefsError(String(e)); }
    setRefsLoading(false);
  };

  return (
    <div className="jb-zotero-item">
      <div className="jb-zotero-item-type">{item.item_type}</div>
      <div className="jb-zotero-item-title">{item.title}</div>
      {(authorStr || item.year) && (
        <div className="jb-zotero-item-meta">
          {authorStr}{authorStr && item.year ? ' · ' : ''}{item.year}
        </div>
      )}
      {item.tags.length > 0 && (
        <div className="jb-zotero-item-tags">
          {item.tags.slice(0, 4).map(t => (
            <span key={t} className="jb-zotero-tag">{t}</span>
          ))}
        </div>
      )}
      <div className="jb-zotero-item-actions">
        <button
          className="jb-zotero-import-btn"
          onClick={() => onImport(item)}
          title="Créer une note paper dans le vault"
        >
          + Importer
        </button>
        <button
          className="jb-zotero-annot-btn"
          onClick={loadAnnotations}
          title="Voir les surlignages du PDF"
        >
          {annotLoading ? '⟳' : annotations !== null ? '▴ Surlignages' : '▾ Surlignages'}
        </button>
        <button
          className="jb-zotero-summary-btn"
          onClick={loadSummary}
          disabled={summaryLoading}
          title={linkedNote
            ? `Résumé approfondi → ajouté à [[${linkedNote.title}]]`
            : 'Résumé approfondi du PDF (Claude) — aucune note liée'}
        >
          {summaryLoading ? '⟳' : summary !== null ? '▴ Résumé' : '◎ Résumé'}
        </button>
        <button
          className="jb-zotero-refs-btn"
          onClick={loadRefs}
          disabled={refsLoading}
          title={linkedNote
            ? `Extraire les références → ajoutées à [[${linkedNote.title}]]`
            : 'Extraire les références bibliographiques — aucune note liée'}
        >
          {refsLoading ? '⟳' : refs !== null ? '▴ Réfs' : '⊞ Réfs'}
        </button>
      </div>

      {annotations !== null && annotations.length === 0 && (
        <div className="jb-zotero-annot-empty">Aucun surlignage (Zotero 6+ requis)</div>
      )}
      {annotations && annotations.length > 0 && (
        <div className="jb-zotero-annotations">
          {annotations.map((a, i) => (
            <div key={i} className="jb-zotero-annot"
              style={{ borderLeftColor: ANNOT_COLOR[a.color ?? ''] ?? 'var(--jb-accent)' }}>
              <span className="jb-zotero-annot-text">"{a.text}"</span>
              {a.page_label && <span className="jb-zotero-annot-page">p.{a.page_label}</span>}
              {a.comment && <div className="jb-zotero-annot-comment">{a.comment}</div>}
            </div>
          ))}
        </div>
      )}

      {summaryError && (
        <div className="jb-zotero-annot-empty" style={{ color: 'var(--jb-error, #e07070)' }}>{summaryError}</div>
      )}
      {summary !== null && !summaryError && (
        <div className="jb-zotero-summary">
          {summary.split('\n').map((line, i) => <p key={i}>{line}</p>)}
        </div>
      )}

      {refsError && (
        <div className="jb-zotero-annot-empty" style={{ color: 'var(--jb-error, #e07070)' }}>{refsError}</div>
      )}
      {refs !== null && !refsError && (
        <div className="jb-zotero-refs">
          {refs.length === 0
            ? <div className="jb-zotero-annot-empty">Aucune référence trouvée</div>
            : refs.map((r, i) => (
                <div key={i} className="jb-zotero-ref-row">
                  <span className="jb-zotero-ref-num">[{i + 1}]</span>
                  <span className="jb-zotero-ref-text">{r}</span>
                </div>
              ))
          }
        </div>
      )}
    </div>
  );
}
