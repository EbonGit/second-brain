import { useState, useEffect, useRef } from 'react';
import { useBrainStore } from '../store';
import { NOTE_TYPE_COLOR, NOTE_TYPE_LABEL, NOTE_TYPES, type NoteType } from '../types';
import './NewNoteModal.css';

export default function NewNoteModal() {
  const { showNewNote, setShowNewNote, createNote } = useBrainStore();
  const [type, setType] = useState<NoteType>('idea');
  const [title, setTitle] = useState('');
  const [creating, setCreating] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (showNewNote) {
      setTitle('');
      setCreating(false);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [showNewNote]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setShowNewNote(false);
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [setShowNewNote]);

  if (!showNewNote) return null;

  const handleCreate = async () => {
    if (!title.trim() || creating) return;
    setCreating(true);
    await createNote(type, title.trim());
    setShowNewNote(false);
  };

  return (
    <div className="jb-modal-overlay" onClick={() => setShowNewNote(false)}>
      <div className="jb-modal" onClick={e => e.stopPropagation()}>
        <div className="jb-modal-header">
          <span>Nouvelle note</span>
          <button className="jb-modal-close" onClick={() => setShowNewNote(false)}>✕</button>
        </div>

        <div className="jb-modal-body">
          <div className="jb-type-grid">
            {NOTE_TYPES.map(t => (
              <button
                key={t}
                className={`jb-type-btn ${type === t ? 'jb-type-btn--active' : ''}`}
                onClick={() => setType(t)}
                style={type === t ? { borderColor: NOTE_TYPE_COLOR[t], background: `${NOTE_TYPE_COLOR[t]}15` } : {}}
              >
                <span
                  className="jb-type-btn-dot"
                  style={{ background: NOTE_TYPE_COLOR[t] }}
                />
                <span style={type === t ? { color: NOTE_TYPE_COLOR[t] } : {}}>
                  {NOTE_TYPE_LABEL[t]}
                </span>
              </button>
            ))}
          </div>

          <input
            ref={inputRef}
            className="jb-modal-input"
            type="text"
            value={title}
            onChange={e => setTitle(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleCreate()}
            placeholder="Titre de la note..."
          />
        </div>

        <div className="jb-modal-footer">
          <button className="jb-modal-btn jb-modal-btn--cancel" onClick={() => setShowNewNote(false)}>
            Annuler
          </button>
          <button
            className="jb-modal-btn jb-modal-btn--create"
            onClick={handleCreate}
            disabled={!title.trim() || creating}
          >
            {creating ? 'Création…' : 'Créer'}
          </button>
        </div>
      </div>
    </div>
  );
}
