import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useBrainStore } from '../store';
import type { Settings } from '../types';
import './SettingsModal.css';

export default function SettingsModal() {
  const { showSettings, setShowSettings, settings, saveSettings } = useBrainStore();
  const [form, setForm] = useState<Settings>({});
  const [saved, setSaved] = useState(false);
  const [detectedDb, setDetectedDb] = useState('');

  useEffect(() => {
    if (showSettings) {
      setForm(settings);
      setSaved(false);
      invoke<string>('detect_zotero_db').then(setDetectedDb).catch(() => setDetectedDb(''));
    }
  }, [showSettings, settings]);

  if (!showSettings) return null;

  const handleSave = async () => {
    await saveSettings(form);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const zoteroDbDisplay = form.zotero_db_path || detectedDb || '…non détecté';
  const zoteroOk = !!(form.zotero_db_path || detectedDb);

  return (
    <div className="jb-modal-overlay" onClick={() => setShowSettings(false)}>
      <div className="jb-settings-modal" onClick={e => e.stopPropagation()}>
        <div className="jb-settings-hdr">
          <span className="jb-settings-title">Réglages</span>
          <button className="jb-settings-close" onClick={() => setShowSettings(false)}>✕</button>
        </div>

        <div className="jb-settings-body">
          {/* Claude */}
          <section className="jb-settings-section">
            <h3>Claude (Anthropic)</h3>
            <p className="jb-settings-hint">
              Pour "Explorer les liens" et le chat vault.
            </p>

            <div className="jb-settings-mode-toggle">
              <button
                className={`jb-settings-mode-btn ${(form.use_claude_cli ?? true) ? 'jb-settings-mode-btn--active' : ''}`}
                onClick={() => setForm(f => ({ ...f, use_claude_cli: true }))}
              >
                Claude Code <span className="jb-settings-mode-tag">abonnement</span>
              </button>
              <button
                className={`jb-settings-mode-btn ${(form.use_claude_cli ?? true) ? '' : 'jb-settings-mode-btn--active'}`}
                onClick={() => setForm(f => ({ ...f, use_claude_cli: false }))}
              >
                Clé API <span className="jb-settings-mode-tag">crédits</span>
              </button>
            </div>

            {(form.use_claude_cli ?? true) ? (
              <div className="jb-settings-hint" style={{ marginTop: 8 }}>
                Utilise ton abonnement Claude.ai — aucun crédit API supplémentaire.<br /><br />
                <strong style={{ color: 'var(--jb-warn)', fontSize: 11 }}>Connexion unique requise :</strong>
                <ol style={{ margin: '6px 0 4px', paddingLeft: 18, fontSize: 11, lineHeight: 1.8 }}>
                  <li>Ouvre <strong>cmd.exe</strong> ou <strong>PowerShell</strong></li>
                  <li>Lance :<br />
                    <code style={{ fontSize: 10, padding: '2px 6px', background: 'var(--jb-bg-canvas)', borderRadius: 3, display: 'inline-block', marginTop: 2, userSelect: 'all' }}>
                      C:\Users\leben\AppData\Roaming\npm\claude.cmd
                    </code>
                  </li>
                  <li>Quand Claude démarre, tape <code style={{ fontSize: 10 }}>/login</code></li>
                  <li>Connecte-toi dans le navigateur</li>
                  <li>Reviens ici et relance l'app</li>
                </ol>
                <span style={{ opacity: 0.6, fontSize: 10 }}>Alternative : utilise le mode Clé API à droite →</span>
              </div>
            ) : (
              <>
                <label className="jb-settings-label" style={{ marginTop: 8 }}>Clé API Anthropic</label>
                <input
                  type="password"
                  className="jb-settings-input"
                  placeholder="sk-ant-api03-…"
                  value={form.anthropic_api_key ?? ''}
                  onChange={e => setForm(f => ({ ...f, anthropic_api_key: e.target.value || undefined }))}
                />
                <p className="jb-settings-hint" style={{ marginTop: 6 }}>
                  Clé sur <strong>console.anthropic.com</strong> → API Keys → Create Key.
                </p>
              </>
            )}
          </section>

          {/* Zotero */}
          <section className="jb-settings-section">
            <h3>Zotero</h3>
            <p className="jb-settings-hint">
              Aucun compte requis — l'app lit directement ta base locale Zotero.
            </p>

            <div className={`jb-settings-detect ${zoteroOk ? 'jb-settings-detect--ok' : 'jb-settings-detect--warn'}`}>
              <span className="jb-settings-detect-dot" />
              <span className="jb-settings-detect-path">{zoteroDbDisplay}</span>
            </div>

            <label className="jb-settings-label" style={{ marginTop: 10 }}>
              Chemin custom vers zotero.sqlite
              <span className="jb-settings-label-hint"> (laisser vide = auto-détecté)</span>
            </label>
            <input
              type="text"
              className="jb-settings-input"
              placeholder={detectedDb || 'C:\\Users\\…\\Zotero\\zotero.sqlite'}
              value={form.zotero_db_path ?? ''}
              onChange={e => setForm(f => ({ ...f, zotero_db_path: e.target.value || undefined }))}
            />
          </section>
        </div>

        <div className="jb-settings-footer">
          <button className="jb-settings-cancel" onClick={() => setShowSettings(false)}>Annuler</button>
          <button className="jb-settings-save" onClick={handleSave}>
            {saved ? '✓ Sauvegardé' : 'Sauvegarder'}
          </button>
        </div>
      </div>
    </div>
  );
}
