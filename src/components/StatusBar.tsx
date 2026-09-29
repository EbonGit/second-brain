import { useBrainStore } from '../store';
import { NOTE_TYPE_COLOR, NOTE_TYPES } from '../types';
import './StatusBar.css';

export default function StatusBar() {
  const { notes, activeNote, vaultPath } = useBrainStore();

  return (
    <div className="jb-statusbar">
      <div className="jb-statusbar-left">
        {NOTE_TYPES.map(type => {
          const count = notes.filter(n => n.note_type === type).length;
          if (count === 0) return null;
          return (
            <span key={type} className="jb-status-chip">
              <span
                className="jb-status-dot-sm"
                style={{ background: NOTE_TYPE_COLOR[type] }}
              />
              {count}
            </span>
          );
        })}
      </div>
      <div className="jb-statusbar-center">
        {activeNote && (
          <span className="jb-status-file">
            {activeNote.meta.path.replace(/\\/g, '/').split('/').slice(-2).join('/')}
            {activeNote.meta.interest != null && activeNote.meta.interest > 0 && (
              <span className="jb-status-stars">
                {'★'.repeat(activeNote.meta.interest)}
              </span>
            )}
          </span>
        )}
      </div>
      <div className="jb-statusbar-right">
        <span className="jb-status-vault">{notes.length} notes</span>
      </div>
    </div>
  );
}
