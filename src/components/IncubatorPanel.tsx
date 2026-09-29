import { useBrainStore } from '../store';
import type { IncubatorNotif } from '../types';
import './IncubatorPanel.css';

interface Props {
  onClose: () => void;
}

export default function IncubatorPanel({ onClose }: Props) {
  const { incubatorNotifs, dismissIncubatorNotif, acceptIncubatorConnection } = useBrainStore();

  return (
    <div className="jb-incubator-panel">
      <div className="jb-incubator-hdr">
        <span className="jb-incubator-badge-dot" />
        <span className="jb-incubator-hdr-title">◉ Connexions suggérées</span>
        <button className="jb-incubator-close" onClick={onClose} title="Fermer">✕</button>
      </div>
      <div className="jb-incubator-body">
        {incubatorNotifs.length === 0 ? (
          <p className="jb-incubator-hint">Aucune suggestion — sauvegarde des notes pour lancer l'analyse.</p>
        ) : (
          incubatorNotifs.map((n: IncubatorNotif, i: number) => (
            <div key={i} className="jb-incubator-card">
              <div className="jb-incubator-card-titles">
                <span className="jb-incubator-card-src">{n.source_title}</span>
                <span className="jb-incubator-card-arrow">→</span>
                <span className="jb-incubator-card-tgt">{n.target_title}</span>
              </div>
              <div className="jb-incubator-card-score">
                <span
                  className="jb-incubator-score-bar"
                  style={{ width: `${Math.round(n.score * 100)}px` }}
                />
                <span className="jb-incubator-score-pct">{Math.round(n.score * 100)}%</span>
              </div>
              <div className="jb-incubator-card-actions">
                <button
                  className="jb-incubator-accept"
                  title="Ajouter [[lien]] dans la note source"
                  onClick={() => acceptIncubatorConnection(n.source_path, n.target_title, n.target_path)}
                >
                  + Connecter
                </button>
                <button
                  className="jb-incubator-dismiss"
                  title="Ignorer cette suggestion"
                  onClick={() => dismissIncubatorNotif(i)}
                >
                  ✕
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
