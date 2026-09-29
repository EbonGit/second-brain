import { useBrainStore } from '../store';
import './DailyReminderModal.css';

export default function DailyReminderModal() {
  const { yesterdayReminder, openNote, notes } = useBrainStore();

  if (!yesterdayReminder) return null;

  const handleOpen = () => {
    // Find yesterday's daily note path
    const note = notes.find(n =>
      n.note_type === 'daily' && n.title === yesterdayReminder
    );
    if (note) openNote(note.path);
    useBrainStore.setState({ yesterdayReminder: null });
  };

  return (
    <div className="jb-daily-reminder-overlay">
      <div className="jb-daily-reminder">
        <div className="jb-daily-reminder-icon">◈</div>
        <p className="jb-daily-reminder-title">Hier tu n'as pas rempli ta journée</p>
        <p className="jb-daily-reminder-sub">
          La section <strong>Ma journée</strong> du {yesterdayReminder} est vide.
          Veux-tu y écrire une réflexion rapide ?
        </p>
        <div className="jb-daily-reminder-actions">
          <button className="jb-daily-reminder-btn jb-daily-reminder-btn--open" onClick={handleOpen}>
            Ouvrir la note
          </button>
          <button
            className="jb-daily-reminder-btn jb-daily-reminder-btn--skip"
            onClick={() => useBrainStore.setState({ yesterdayReminder: null })}
          >
            Ignorer
          </button>
        </div>
      </div>
    </div>
  );
}
