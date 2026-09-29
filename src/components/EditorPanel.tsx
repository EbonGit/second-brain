import { useState, useEffect, useCallback, useRef } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { useBrainStore } from '../store';
import { NOTE_TYPE_COLOR } from '../types';
import './EditorPanel.css';
import './ExploreDrawer.css';

/** Render text with [[wikilinks]] as clickable chips. */
function WikiText({ text, onLinkClick }: { text: string; onLinkClick: (title: string) => void }) {
  const parts = text.split(/(\[\[[^\]]+\]\])/g);
  return (
    <>
      {parts.map((part, i) => {
        const m = part.match(/^\[\[(.+)\]\]$/);
        if (m) {
          return (
            <button
              key={i}
              className="jb-explore-wikilink"
              title={`Ajouter [[${m[1]}]] à la note`}
              onClick={() => onLinkClick(m[1])}
            >
              [[{m[1]}]]
            </button>
          );
        }
        return <ReactMarkdown key={i} remarkPlugins={[remarkGfm]}>{part}</ReactMarkdown>;
      })}
    </>
  );
}

type ViewMode = 'split' | 'source' | 'preview';

export default function EditorPanel() {
  const {
    activeNote, saveNote, deleteNote,
    exploreLinks, exploreResult, exploreLoading, clearExplore, addLinkToNote, claudeUsage,
    suggestedTags, suggestTagsLoading, suggestTagsMsg, fetchSuggestedTags, applyTags, clearSuggestedTags,
    feynmanQuestion, feynmanResult, feynmanLoading, clearFeynman,
    detectContradictions, contradictionResult, contradictionLoading, clearContradictions,
  } = useBrainStore();
  const [content, setContent] = useState('');
  const [isDirty, setIsDirty] = useState(false);
  const [viewMode, setViewMode] = useState<ViewMode>('split');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (activeNote) {
      setContent(activeNote.raw);
      setIsDirty(false);
      clearSuggestedTags();
    }
  }, [activeNote?.meta.id]);

  const handleSave = useCallback(async () => {
    if (activeNote && isDirty) {
      await saveNote(activeNote.meta.path, content);
      setIsDirty(false);
      // Ask Ollama for tag suggestions after save (background, non-blocking)
      fetchSuggestedTags(activeNote.meta.path).catch(() => {});
    }
  }, [activeNote, content, isDirty, saveNote, fetchSuggestedTags]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        handleSave();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'n') {
        e.preventDefault();
        useBrainStore.getState().setShowNewNote(true);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [handleSave]);

  if (!activeNote) {
    return (
      <div className="jb-editor-empty">
        <span className="jb-editor-empty-icon">◈</span>
        <p>Sélectionne une note ou crée-en une nouvelle</p>
        <p className="jb-editor-empty-hint">Ctrl+N pour créer · Ctrl+S pour sauvegarder</p>
      </div>
    );
  }

  const typeColor = NOTE_TYPE_COLOR[activeNote.meta.note_type];

  // Extract frontmatter block to show separately
  const rawLines = content.split('\n');
  const hasFm = content.startsWith('---');
  let fmEnd = -1;
  if (hasFm) {
    const after = rawLines.slice(1);
    const idx = after.findIndex(l => l.trim() === '---');
    if (idx !== -1) fmEnd = idx + 2;
  }
  // Live body for preview — strip frontmatter from the textarea content so the
  // preview updates as the user types without waiting for a save round-trip.
  const previewBody = (hasFm && fmEnd > 0)
    ? rawLines.slice(fmEnd).join('\n').trim()
    : content;

  return (
    <div className="jb-editor">
      {/* Toolbar */}
      <div className="jb-editor-toolbar">
        <span className="jb-editor-type-dot" style={{ background: typeColor }} />
        <span className="jb-editor-title">{activeNote.meta.title}</span>
        {activeNote.meta.tags.length > 0 && (
          <div className="jb-editor-tags">
            {activeNote.meta.tags.map(t => (
              <span key={t} className="jb-editor-tag">{t}</span>
            ))}
          </div>
        )}
        <div className="jb-editor-spacer" />
        <div className="jb-editor-viewmode">
          {(['split', 'source', 'preview'] as ViewMode[]).map(m => (
            <button
              key={m}
              className={`jb-viewmode-btn ${viewMode === m ? 'jb-viewmode-btn--active' : ''}`}
              onClick={() => setViewMode(m)}
            >
              {m === 'split' ? '⊞' : m === 'source' ? '</>' : '◉'}
            </button>
          ))}
        </div>
        <button
          className="jb-tags-btn"
          onClick={() => activeNote && fetchSuggestedTags(activeNote.meta.path)}
          disabled={suggestTagsLoading}
          title="Suggérer des tags via Ollama"
        >
          {suggestTagsLoading ? '⟳ Tags…' : '∿ Tags'}
        </button>
        <button
          className="jb-explore-btn"
          onClick={exploreLinks}
          disabled={exploreLoading}
          title="Demander à Claude de trouver des liens non-évidents"
        >
          {exploreLoading ? '⟳ Explorer…' : '◈ Explorer les liens'}
        </button>
        <button
          className="jb-feynman-btn"
          onClick={feynmanQuestion}
          disabled={feynmanLoading}
          title="5 questions de Feynman (Claude)"
        >
          {feynmanLoading ? '⟳ Feynman…' : '? Feynman'}
        </button>
        <button
          className="jb-contradiction-btn"
          onClick={detectContradictions}
          disabled={contradictionLoading}
          title="Détecter les tensions et contradictions (Claude)"
        >
          {contradictionLoading ? '⟳ Tensions…' : 'Tensions'}
        </button>
        <button
          className={`jb-save-btn ${isDirty ? 'jb-save-btn--dirty' : ''}`}
          onClick={handleSave}
          disabled={!isDirty}
        >
          {isDirty ? '● Sauvegarder' : '✓ Sauvegardé'}
        </button>
      </div>

      {(suggestedTags.length > 0 || suggestTagsLoading || suggestTagsMsg !== null) && (
        <div className="jb-tag-suggest-bar">
          {suggestTagsMsg === 'no_model' ? (
            <>
              <span className="jb-tag-suggest-label jb-tag-suggest-label--warn">
                ∿ Aucun modèle Ollama génératif — installe-en un :
              </span>
              <code className="jb-tag-suggest-cmd">ollama pull llama3.2</code>
            </>
          ) : suggestTagsMsg === 'error' ? (
            <span className="jb-tag-suggest-label jb-tag-suggest-label--warn">
              ∿ Ollama inaccessible
            </span>
          ) : (
            <>
              <span className="jb-tag-suggest-label">
                {suggestTagsLoading ? '∿ Ollama suggère…' : '∿ Tags suggérés :'}
              </span>
              {suggestedTags.map(tag => (
                <button
                  key={tag}
                  className="jb-tag-suggest-pill"
                  title={`Ajouter le tag "${tag}"`}
                  onClick={() => activeNote && applyTags(activeNote.meta.path, [tag])}
                >
                  + {tag}
                </button>
              ))}
              {suggestedTags.length > 1 && activeNote && (
                <button
                  className="jb-tag-suggest-pill jb-tag-suggest-pill--all"
                  title="Ajouter tous les tags suggérés"
                  onClick={() => applyTags(activeNote.meta.path, suggestedTags)}
                >
                  + Tous
                </button>
              )}
            </>
          )}
          <button className="jb-tag-suggest-dismiss" onClick={clearSuggestedTags} title="Ignorer">✕</button>
        </div>
      )}

      {(exploreResult || exploreLoading) && (
        <div className="jb-explore-drawer">
          <div className="jb-explore-drawer-hdr">
            <span>◈ Connexions suggérées par Claude</span>
            <div className="jb-explore-drawer-hdr-right">
              {claudeUsage && !exploreLoading && (
                <span className="jb-token-usage" title="Tokens consommés pour cette requête">
                  ↑{claudeUsage.input} ↓{claudeUsage.output} tok
                </span>
              )}
              <button onClick={clearExplore} disabled={exploreLoading}>✕</button>
            </div>
          </div>
          <div className="jb-explore-drawer-body">
            {exploreLoading ? (
              <div className="jb-explore-thinking"><span /><span /><span /></div>
            ) : exploreResult && (
              <>
                <WikiText
                  text={exploreResult}
                  onLinkClick={title => activeNote && addLinkToNote(activeNote.meta.path, title)}
                />
                {activeNote && (
                  <p className="jb-explore-hint">Clique sur [[un lien]] pour l'ajouter à la note</p>
                )}
              </>
            )}
          </div>
        </div>
      )}

      {(feynmanResult || feynmanLoading) && (
        <div className="jb-explore-drawer">
          <div className="jb-explore-drawer-hdr">
            <span>? Questions de Feynman</span>
            <div className="jb-explore-drawer-hdr-right">
              {claudeUsage && !feynmanLoading && (
                <span className="jb-token-usage">↑{claudeUsage.input} ↓{claudeUsage.output} tok</span>
              )}
              <button onClick={clearFeynman} disabled={feynmanLoading}>✕</button>
            </div>
          </div>
          <div className="jb-explore-drawer-body">
            {feynmanLoading ? (
              <div className="jb-explore-thinking"><span /><span /><span /></div>
            ) : feynmanResult && (
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{feynmanResult}</ReactMarkdown>
            )}
          </div>
        </div>
      )}

      {(contradictionResult || contradictionLoading) && (
        <div className="jb-explore-drawer">
          <div className="jb-explore-drawer-hdr">
            <span>Tensions détectées</span>
            <div className="jb-explore-drawer-hdr-right">
              {claudeUsage && !contradictionLoading && (
                <span className="jb-token-usage">↑{claudeUsage.input} ↓{claudeUsage.output} tok</span>
              )}
              <button onClick={clearContradictions} disabled={contradictionLoading}>✕</button>
            </div>
          </div>
          <div className="jb-explore-drawer-body">
            {contradictionLoading ? (
              <div className="jb-explore-thinking"><span /><span /><span /></div>
            ) : contradictionResult && (
              <WikiText
                text={contradictionResult}
                onLinkClick={title => activeNote && addLinkToNote(activeNote.meta.path, title)}
              />
            )}
          </div>
        </div>
      )}

      {/* Content */}
      <div className="jb-editor-body">
        {(viewMode === 'split' || viewMode === 'source') && (
          <div className={`jb-editor-source-pane ${viewMode === 'source' ? 'jb-editor-pane--full' : ''}`}>
            <textarea
              ref={textareaRef}
              className="jb-source-textarea"
              value={content}
              onChange={e => { setContent(e.target.value); setIsDirty(true); }}
              spellCheck={false}
              placeholder="Commence à écrire en Markdown..."
            />
          </div>
        )}
        {(viewMode === 'split' || viewMode === 'preview') && (
          <div className={`jb-editor-preview-pane ${viewMode === 'preview' ? 'jb-editor-pane--full' : ''}`}>
            <div className="jb-markdown">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {previewBody || '*Pas de contenu*'}
              </ReactMarkdown>
            </div>
          </div>
        )}
      </div>

      {/* Meta footer */}
      <div className="jb-editor-footer">
        <span className="jb-editor-meta">
          Créé : {activeNote.meta.created}
        </span>
        {activeNote.meta.status && (
          <span className="jb-editor-meta">· {activeNote.meta.status}</span>
        )}
        {activeNote.meta.interest != null && activeNote.meta.interest > 0 && (
          <span className="jb-editor-meta jb-editor-stars">
            · {'★'.repeat(activeNote.meta.interest)}
          </span>
        )}
        <div className="jb-editor-spacer" />
        <button
          className="jb-delete-btn"
          onClick={() => {
            if (confirm(`Supprimer "${activeNote.meta.title}" ?`))
              deleteNote(activeNote.meta.path);
          }}
        >
          Supprimer
        </button>
      </div>
    </div>
  );
}
