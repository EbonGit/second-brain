import { useState, useEffect, useRef } from 'react';
import { useBrainStore } from '../store';
import { NOTE_TYPE_COLOR, NOTE_TYPE_LABEL, NOTE_TYPES, type NoteType, type NoteMeta } from '../types';
import './NoteListPanel.css';

const STATUS_CYCLE: Record<string, string> = {
  'non-lu': 'en-cours',
  'en-cours': 'lu',
  'lu': 'non-lu',
};
const STATUS_LABEL: Record<string, string> = {
  'non-lu': '○',
  'en-cours': '◑',
  'lu': '●',
};
const STATUS_TITLE: Record<string, string> = {
  'non-lu': 'Non lu → En cours',
  'en-cours': 'En cours → Lu',
  'lu': 'Lu → Non lu',
};

function maturityInfo(score: number) {
  if (score >= 65) return { color: 'var(--jb-idea)',        title: `Mature (${score})` };
  if (score >= 30) return { color: '#c9a227',               title: `Developing (${score})` };
  return               { color: 'var(--jb-text-disabled)',   title: `Seed (${score})` };
}

function NoteRow({
  note, isActive, selectMode, isSelected,
}: {
  note: NoteMeta; isActive: boolean;
  selectMode: boolean; isSelected: boolean;
}) {
  const { openNote, updateNoteMeta, toggleNoteSelection } = useBrainStore();
  const interest = note.interest ?? 0;
  const status = note.status ?? 'non-lu';
  const mat = maturityInfo(note.maturity_score ?? 0);

  const setInterest = (e: React.MouseEvent, val: number) => {
    e.stopPropagation();
    updateNoteMeta(note.path, val === interest ? 0 : val, undefined);
  };

  const cycleStatus = (e: React.MouseEvent) => {
    e.stopPropagation();
    const next = STATUS_CYCLE[status] ?? 'non-lu';
    updateNoteMeta(note.path, undefined, next);
  };

  const handleClick = () => {
    if (selectMode) toggleNoteSelection(note.path);
    else openNote(note.path);
  };

  return (
    <button
      className={`jb-note-row ${isActive ? 'jb-note-row--active' : ''} ${isSelected ? 'jb-note-row--selected' : ''}`}
      onClick={handleClick}
    >
      {selectMode ? (
        <span className="jb-note-checkbox" style={{ color: isSelected ? 'var(--jb-accent)' : 'var(--jb-text-disabled)' }}>
          {isSelected ? '☑' : '☐'}
        </span>
      ) : (
        <button
          className="jb-status-cycle"
          title={STATUS_TITLE[status] ?? 'Changer statut'}
          onClick={cycleStatus}
          style={{ color: status === 'non-lu' ? 'var(--jb-text-disabled)' : NOTE_TYPE_COLOR[note.note_type] }}
        >
          {STATUS_LABEL[status] ?? '○'}
        </button>
      )}

      <span
        className="jb-maturity-dot"
        style={{ background: mat.color }}
        title={mat.title}
      />

      <span className="jb-note-title">{note.title}</span>

      {!selectMode && (
        <span className="jb-note-stars-edit" onClick={e => e.stopPropagation()}>
          {[1, 2, 3, 4, 5].map(i => (
            <button
              key={i}
              className={`jb-star ${i <= interest ? 'jb-star--on' : ''}`}
              title={`Intérêt ${i}`}
              onClick={e => setInterest(e, i)}
            >
              {i <= interest ? '★' : '☆'}
            </button>
          ))}
        </span>
      )}
    </button>
  );
}

export default function NoteListPanel() {
  const {
    notes, activeNote, setShowNewNote, loadVault, openNote,
    ghostLinks, fetchGhostLinks,
    pdfSearchResults, pdfSearchLoading, pdfIndexStatus, searchPdfFulltext, buildPdfIndex,
    semanticResults, semanticLoading, searchSemantic,
    selectedNotes, toggleNoteSelection, clearNoteSelection, generateTangle, tangleLoading,
    generateGhostNote, ghostGenerating,
  } = useBrainStore();
  const [expanded, setExpanded] = useState<Set<NoteType>>(new Set(['paper', 'idea']));
  const [ghostExpanded, setGhostExpanded] = useState(false);
  const [search, setSearch] = useState('');
  const [pdfMode, setPdfMode] = useState(false);
  const [semanticMode, setSemanticMode] = useState(false);
  const [selectMode, setSelectMode] = useState(false);
  const [pdfBuilding, setPdfBuilding] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Load ghost links once at mount
  useEffect(() => { fetchGhostLinks(); }, []);

  const toggle = (type: NoteType) =>
    setExpanded(prev => { const s = new Set(prev); s.has(type) ? s.delete(type) : s.add(type); return s; });

  const handleSearch = (value: string) => {
    setSearch(value);
    if (pdfMode) {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => searchPdfFulltext(value), 300);
    } else if (semanticMode) {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => searchSemantic(value), 400);
    }
  };

  const handleBuildIndex = async () => {
    setPdfBuilding(true);
    await buildPdfIndex();
    setPdfBuilding(false);
  };

  const pdfStatusColor = pdfIndexStatus?.startsWith('✓') ? 'var(--jb-idea)' : 'var(--jb-error, #e07070)';

  const filtered = !pdfMode && !semanticMode && search.trim()
    ? notes.filter(n =>
        n.title.toLowerCase().includes(search.toLowerCase()) ||
        n.tags.some(t => t.toLowerCase().includes(search.toLowerCase()))
      )
    : null;

  return (
    <div className="jb-note-list">
      <div className="jb-note-list-toolbar">
        <input
          className="jb-search"
          placeholder={
            pdfMode ? 'Chercher dans les PDFs…'
            : semanticMode ? 'Recherche sémantique (Ollama)…'
            : 'Rechercher…'
          }
          value={search}
          onChange={e => handleSearch(e.target.value)}
        />
        <button
          className={`jb-icon-btn jb-icon-btn--semantic ${semanticMode ? 'jb-icon-btn--semantic-active' : ''}`}
          onClick={() => { setSemanticMode(p => !p); setPdfMode(false); setSearch(''); }}
          title="Recherche sémantique (Ollama)"
        >
          ∿
        </button>
        <button
          className={`jb-icon-btn jb-icon-btn--pdf ${pdfMode ? 'jb-icon-btn--pdf-active' : ''}`}
          onClick={() => { setPdfMode(p => !p); setSemanticMode(false); setSearch(''); }}
          title="Recherche full-text PDF"
        >
          ⊡
        </button>
        <button
          className={`jb-icon-btn ${selectMode ? 'jb-icon-btn--select-active' : ''}`}
          onClick={() => { setSelectMode(p => !p); clearNoteSelection(); }}
          title={selectMode ? 'Quitter la sélection' : 'Sélectionner des notes (Tangle)'}
        >
          ⊕
        </button>
        <button className="jb-icon-btn" onClick={() => loadVault()} title="Rafraîchir">↺</button>
        <button className="jb-icon-btn jb-icon-btn--accent" onClick={() => setShowNewNote(true)} title="Nouvelle note">+</button>
      </div>

      <div className="jb-note-list-body">
        {/* Semantic search mode */}
        {semanticMode && (
          <>
            {semanticLoading && <p className="jb-empty">Recherche sémantique…</p>}
            {!semanticLoading && semanticResults.length === 0 && search && (
              <p className="jb-empty">Aucun résultat sémantique — Ollama disponible ?</p>
            )}
            {!semanticLoading && !search && (
              <p className="jb-empty" style={{ fontStyle: 'normal' }}>∿ Tape une idée, un concept…</p>
            )}
            <div className="jb-semantic-results">
              {semanticResults.map((r, i) => (
                <button
                  key={i}
                  className="jb-semantic-result"
                  onClick={() => openNote(r.path)}
                >
                  <div className="jb-semantic-result-title">{r.title}</div>
                  <div className="jb-semantic-result-score">
                    <span
                      className="jb-semantic-score-bar"
                      style={{ width: `${Math.round(r.score * 100)}%` }}
                    />
                    <span className="jb-semantic-score-pct">{Math.round(r.score * 100)}%</span>
                  </div>
                </button>
              ))}
            </div>
          </>
        )}

        {/* PDF search mode */}
        {pdfMode && (
          <>
            <button className="jb-pdf-build-btn" onClick={handleBuildIndex} disabled={pdfBuilding}>
              {pdfBuilding ? '⟳ Indexation…' : '⊡ Indexer les PDFs Zotero'}
            </button>
            {pdfIndexStatus && (
              <p className="jb-empty" style={{ color: pdfStatusColor, fontStyle: 'normal' }}>
                {pdfIndexStatus}
              </p>
            )}
            {pdfSearchLoading && <p className="jb-empty">Recherche…</p>}
            {!pdfSearchLoading && pdfSearchResults.length === 0 && search && (
              <p className="jb-empty">Aucun résultat PDF</p>
            )}
            <div className="jb-pdf-results">
              {pdfSearchResults.map((r, i) => {
                const linkedNote = notes.find(n =>
                  n.title.toLowerCase() === r.title.toLowerCase()
                );
                return (
                  <div
                    key={i}
                    className={`jb-pdf-result ${linkedNote ? 'jb-pdf-result--linked' : ''}`}
                    onClick={() => linkedNote && openNote(linkedNote.path)}
                    title={linkedNote ? `Ouvrir : ${linkedNote.title}` : undefined}
                  >
                    <div className="jb-pdf-result-title">
                      {linkedNote && <span className="jb-pdf-result-link-icon">◈ </span>}
                      {r.title}
                    </div>
                    <div className="jb-pdf-result-excerpt">{r.excerpt}</div>
                  </div>
                );
              })}
            </div>
          </>
        )}

        {/* Regular note list */}
        {!pdfMode && !semanticMode && (
          <>
            {filtered ? (
              filtered.length === 0 ? (
                <p className="jb-empty">Aucun résultat</p>
              ) : (
                filtered.map(note => (
                  <NoteRow key={note.id} note={note} isActive={activeNote?.meta.id === note.id}
                    selectMode={selectMode} isSelected={selectedNotes.includes(note.path)} />
                ))
              )
            ) : (
              <>
                {NOTE_TYPES.map(type => {
                  const typeNotes = notes.filter(n => n.note_type === type);
                  const isOpen = expanded.has(type);
                  return (
                    <div key={type} className="jb-type-group">
                      <button className="jb-type-header" onClick={() => toggle(type)}>
                        <span className="jb-type-dot" style={{ background: NOTE_TYPE_COLOR[type] }} />
                        <span className="jb-type-label">{NOTE_TYPE_LABEL[type]}</span>
                        <span className="jb-type-count">{typeNotes.length}</span>
                        <span className="jb-type-chevron">{isOpen ? '▾' : '▸'}</span>
                      </button>
                      {isOpen && (
                        <div className="jb-type-notes">
                          {typeNotes.length === 0 ? (
                            <p className="jb-empty">Aucune note</p>
                          ) : (
                            typeNotes.map(note => (
                              <NoteRow key={note.id} note={note} isActive={activeNote?.meta.id === note.id}
                                selectMode={selectMode} isSelected={selectedNotes.includes(note.path)} />
                            ))
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}

                {/* Ghost links section */}
                {ghostLinks.length > 0 && (
                  <div className="jb-ghost-section">
                    <button className="jb-ghost-header" onClick={() => setGhostExpanded(p => !p)}>
                      <span style={{ opacity: 0.4 }}>◌</span>
                      <span style={{ flex: 1, textAlign: 'left' }}>Fantômes</span>
                      <span className="jb-type-count">{ghostLinks.length}</span>
                      <span className="jb-type-chevron">{ghostExpanded ? '▾' : '▸'}</span>
                    </button>
                    {ghostExpanded && ghostLinks.map(g => (
                      <div key={g.title} className="jb-ghost-row" title={`Cité par : ${g.cited_by.join(', ')}`}>
                        <span style={{ fontSize: 10, opacity: 0.5 }}>◌</span>
                        <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {g.title}
                        </span>
                        <span className="jb-ghost-count">×{g.cited_by.length}</span>
                        <button
                          className="jb-ghost-generate-btn"
                          title={`Générer une note pour «${g.title}» (Claude)`}
                          disabled={ghostGenerating === g.title}
                          onClick={e => { e.stopPropagation(); generateGhostNote(g.title, g.cited_by); }}
                        >
                          {ghostGenerating === g.title ? '⟳' : '+'}
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </>
            )}
          </>
        )}
      </div>

      {/* D5 — Tangle bar: shown when ≥2 notes are selected */}
      {selectMode && selectedNotes.length >= 2 && (
        <div className="jb-tangle-bar">
          <span className="jb-tangle-count">{selectedNotes.length} notes</span>
          <button
            className="jb-tangle-btn"
            onClick={generateTangle}
            disabled={tangleLoading}
            title="Synthétiser en Tangle (Claude)"
          >
            {tangleLoading ? '⟳ Tangle…' : '⊕ Générer Tangle'}
          </button>
          <button
            className="jb-tangle-cancel"
            onClick={() => { clearNoteSelection(); setSelectMode(false); }}
            title="Annuler"
          >
            ✕
          </button>
        </div>
      )}
    </div>
  );
}
