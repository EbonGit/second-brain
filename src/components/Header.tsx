import { useState, useEffect, useRef } from 'react';
import { useBrainStore } from '../store';
import type { VaultEntry } from '../types';
import './Header.css';

export default function Header() {
  const { notes, vaultPath, setShowNewNote, setShowSettings } = useBrainStore();

  const paperCount = notes.filter(n => n.note_type === 'paper').length;

  return (
    <header className="jb-header">
      <div className="jb-header-left">
        <div className="jb-logo">
          <span className="jb-logo-mark">◈</span>
          <span className="jb-logo-text">Jad's Brain</span>
        </div>
        <div className="jb-header-divider" />
        <span className="jb-header-stat">
          <span className="jb-header-stat-num">{notes.length}</span> notes
        </span>
        <span className="jb-header-stat">
          <span className="jb-header-stat-num">{paperCount}</span> papers
        </span>
      </div>

      <div className="jb-header-center">
        <VaultSwitcher />
      </div>

      <div className="jb-header-right">
        <button
          className="jb-btn-new"
          onClick={() => setShowNewNote(true)}
          title="Nouvelle note (Ctrl+N)"
        >
          + Nouvelle note
        </button>
        <button
          className="jb-btn-settings"
          onClick={() => setShowSettings(true)}
          title="Réglages (Zotero, Claude…)"
        >
          ⚙
        </button>
        <div className="jb-header-divider" />
        <OllamaStatus />
      </div>
    </header>
  );
}

function VaultSwitcher() {
  const { vaults, vaultPath, switchVault, addVault, renameVault, removeVault } = useBrainStore();
  const [open, setOpen] = useState(false);
  const [addMode, setAddMode] = useState(false);
  const [newName, setNewName] = useState('');
  const [newPath, setNewPath] = useState('');
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameVal, setRenameVal] = useState('');
  const [switching, setSwitching] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Close popup on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    if (open) document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  const activeVault = vaults.find(v => v.path === vaultPath);
  const displayName = activeVault?.name ?? vaultPath.split(/[\\/]/).slice(-2, -1)[0] ?? 'Vault';

  const handleSwitch = async (v: VaultEntry) => {
    if (v.path === vaultPath || switching) return;
    setSwitching(true);
    setOpen(false);
    await switchVault(v.path);
  };

  const handleAdd = async () => {
    if (!newPath.trim()) return;
    const name = newName.trim() || newPath.trim().split(/[\\/]/).slice(-2, -1)[0] || 'Vault';
    await addVault(name, newPath.trim());
    setNewName(''); setNewPath(''); setAddMode(false);
  };

  const handleRename = async (path: string) => {
    if (renameVal.trim()) await renameVault(path, renameVal.trim());
    setRenaming(null); setRenameVal('');
  };

  const handleRemove = async (v: VaultEntry) => {
    if (vaults.length <= 1) return;
    if (!confirm(`Retirer «${v.name}» de la liste ? (les fichiers ne seront pas supprimés)`)) return;
    await removeVault(v.path);
  };

  return (
    <div className="jb-vault-switcher" ref={ref}>
      <button
        className="jb-vault-switcher-btn"
        onClick={() => setOpen(o => !o)}
        title="Changer de vault"
        disabled={switching}
      >
        <span className="jb-vault-switcher-icon">⊙</span>
        <span className="jb-vault-switcher-name">{displayName}</span>
        <span className="jb-vault-switcher-caret">{open ? '▴' : '▾'}</span>
      </button>

      {open && (
        <div className="jb-vault-popup">
          <div className="jb-vault-popup-title">Vaults</div>

          {vaults.map(v => (
            <div key={v.path} className={`jb-vault-row ${v.path === vaultPath ? 'jb-vault-row--active' : ''}`}>
              {renaming === v.path ? (
                <input
                  className="jb-vault-rename-input"
                  value={renameVal}
                  autoFocus
                  onChange={e => setRenameVal(e.target.value)}
                  onKeyDown={e => {
                    if (e.key === 'Enter') handleRename(v.path);
                    if (e.key === 'Escape') { setRenaming(null); setRenameVal(''); }
                  }}
                  onBlur={() => handleRename(v.path)}
                />
              ) : (
                <button
                  className="jb-vault-row-name"
                  onClick={() => handleSwitch(v)}
                  title={v.path}
                >
                  {v.path === vaultPath && <span className="jb-vault-active-dot" />}
                  {v.name}
                </button>
              )}
              <div className="jb-vault-row-actions">
                <button
                  className="jb-vault-action-btn"
                  title="Renommer"
                  onClick={() => { setRenaming(v.path); setRenameVal(v.name); }}
                >✎</button>
                {vaults.length > 1 && (
                  <button
                    className="jb-vault-action-btn jb-vault-action-btn--remove"
                    title="Retirer de la liste"
                    onClick={() => handleRemove(v)}
                  >✕</button>
                )}
              </div>
            </div>
          ))}

          {addMode ? (
            <div className="jb-vault-add-form">
              <input
                className="jb-vault-add-input"
                placeholder="Nom (optionnel)"
                value={newName}
                onChange={e => setNewName(e.target.value)}
              />
              <input
                className="jb-vault-add-input"
                placeholder="Chemin du dossier vault"
                value={newPath}
                autoFocus
                onChange={e => setNewPath(e.target.value)}
                onKeyDown={e => {
                  if (e.key === 'Enter') handleAdd();
                  if (e.key === 'Escape') setAddMode(false);
                }}
              />
              <div className="jb-vault-add-actions">
                <button className="jb-vault-add-confirm" onClick={handleAdd}>Ajouter</button>
                <button className="jb-vault-add-cancel" onClick={() => setAddMode(false)}>Annuler</button>
              </div>
            </div>
          ) : (
            <button className="jb-vault-add-btn" onClick={() => setAddMode(true)}>
              + Nouveau vault
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function OllamaStatus() {
  const [open, setOpen] = useState(false);
  const [online, setOnline] = useState<boolean | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const [models, setModels] = useState<string[]>([]);

  useEffect(() => {
    const check = async () => {
      try {
        const res = await fetch('http://127.0.0.1:11434/api/version', {
          signal: AbortSignal.timeout(2000),
        });
        if (res.ok) {
          const data = await res.json() as { version?: string };
          setVersion(data.version ?? null);
          setOnline(true);
          // Fetch model list while we're at it
          fetch('http://127.0.0.1:11434/api/tags', { signal: AbortSignal.timeout(3000) })
            .then(r => r.json())
            .then((d: { models?: { name: string }[] }) => {
              setModels((d.models ?? []).map(m => m.name));
            })
            .catch(() => {});
        } else {
          setOnline(false);
        }
      } catch {
        setOnline(false);
        setVersion(null);
        setModels([]);
      }
    };
    check();
    const id = setInterval(check, 15000);
    return () => clearInterval(id);
  }, []);

  const dotClass = online === null
    ? 'jb-status-dot--warn'
    : online
      ? 'jb-status-dot--on'
      : 'jb-status-dot--off';

  const hasEmbed = models.some(m => m.startsWith('nomic-embed-text'));

  return (
    <div className="jb-ollama-status">
      <button
        className="jb-ollama-btn"
        onClick={() => setOpen(v => !v)}
        title={online ? `Ollama actif${version ? ' v' + version : ''}` : 'Ollama hors ligne — cliquer pour les instructions'}
      >
        <div className={`jb-status-dot ${dotClass}`} />
        <span>Ollama</span>
      </button>

      {open && (
        <div className="jb-ollama-popup">
          <button className="jb-ollama-popup-close" onClick={() => setOpen(false)}>✕</button>
          <div className="jb-ollama-popup-title">
            {online ? `Ollama actif ✓${version ? '  ·  v' + version : ''}` : 'Ollama hors ligne'}
          </div>

          {online ? (
            <>
              <div className="jb-ollama-models">
                {models.length === 0 ? (
                  <div className="jb-ollama-model-row jb-ollama-model--warn">
                    <div className="jb-status-dot jb-status-dot--warn" />
                    <span>Aucun modèle installé</span>
                  </div>
                ) : models.map(m => (
                  <div key={m} className={`jb-ollama-model-row ${m.startsWith('nomic-embed-text') ? 'jb-ollama-model--ok' : ''}`}>
                    <div className={`jb-status-dot ${m.startsWith('nomic-embed-text') ? 'jb-status-dot--on' : 'jb-status-dot--warn'}`} />
                    <span>{m}</span>
                  </div>
                ))}
              </div>
              {!hasEmbed && (
                <div className="jb-ollama-popup-note jb-ollama-popup-note--warn">
                  Installe <code>nomic-embed-text</code> pour l'embedding local :<br />
                  <code>ollama pull nomic-embed-text</code>
                </div>
              )}
              <div className="jb-ollama-popup-note">
                {hasEmbed
                  ? '✓ Les liens du graphe utilisent les embeddings Ollama (nomic-embed-text).'
                  : 'TF-IDF actif. Installe nomic-embed-text pour des liens sémantiques plus précis.'}
              </div>
            </>
          ) : (
            <>
              <p className="jb-ollama-popup-desc">
                Ollama n'est pas détecté sur localhost:11434.
              </p>
              <ol className="jb-ollama-popup-steps">
                <li>Installer : <strong>ollama.com/download</strong></li>
                <li>Démarrer :<br /><code>ollama serve</code></li>
                <li>Modèle embedding :<br /><code>ollama pull nomic-embed-text</code></li>
              </ol>
              <div className="jb-ollama-popup-note">
                Sans Ollama, les liens TF-IDF restent actifs.
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
