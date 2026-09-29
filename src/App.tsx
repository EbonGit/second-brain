import { useEffect } from 'react';
import { useBrainStore } from './store';
import Header from './components/Header';
import TileLayout from './components/TileLayout';
import StatusBar from './components/StatusBar';
import NewNoteModal from './components/NewNoteModal';
import SettingsModal from './components/SettingsModal';
import DailyReminderModal from './components/DailyReminderModal';

export default function App() {
  const { initVault, finalizeDailyNote } = useBrainStore();

  useEffect(() => {
    initVault();
    const handleClose = () => { finalizeDailyNote(); };
    window.addEventListener('beforeunload', handleClose);
    return () => window.removeEventListener('beforeunload', handleClose);
  }, []);

  return (
    <div className="jb-app">
      <Header />
      <div className="jb-workspace">
        <TileLayout />
      </div>
      <StatusBar />
      <NewNoteModal />
      <SettingsModal />
      <DailyReminderModal />
    </div>
  );
}
