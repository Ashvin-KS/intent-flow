import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { HomePage } from './components/Home/HomePage';
import { ChatPage } from './components/Chat/ChatPage';
import { Timeline } from './components/Timeline/Timeline';
import { PersonalDashboard } from './components/Dashboard/PersonalDashboard';
import { SettingsModal } from './components/Settings/SettingsModal';
import { AppShell } from './components/Layout/AppShell';

export type PageType = 'home' | 'chat' | 'timeline' | 'workflows' | 'settings';
const PAGE_STORAGE_KEY = 'intentflow_active_page';

function loadInitialPage(): PageType {
  try {
    const raw = localStorage.getItem(PAGE_STORAGE_KEY);
    if (raw === 'home' || raw === 'chat' || raw === 'timeline' || raw === 'workflows') {
      return raw;
    }
  } catch {
    // ignore storage errors
  }
  return 'home';
}

function App() {
  const [activePage, setActivePage] = useState<PageType>(loadInitialPage);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [chatPrompt, setChatPrompt] = useState<string | undefined>();

  const handleNavigate = (page: PageType) => {
    if (page === 'settings') {
      setSettingsOpen(true);
    } else {
      setActivePage(page);
      if (page !== 'chat') {
        setChatPrompt(undefined);
      }
    }
  };

  const handleChatWithPrompt = (prompt: string) => {
    setChatPrompt(prompt);
    setActivePage('chat');
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const setup = async () => {
      unlisten = await listen<string>('tray:navigate', (event) => {
        const page = event.payload;
        if (page === 'chat') {
          setActivePage('chat');
          return;
        }
        if (page === 'home') {
          setActivePage('home');
          return;
        }
      });
    };
    setup();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  useEffect(() => {
    if (activePage === 'settings') return;
    try {
      localStorage.setItem(PAGE_STORAGE_KEY, activePage);
    } catch {
      // ignore storage errors
    }
  }, [activePage]);

  return (
    <>
      <AppShell
        activePage={activePage}
        onNavigate={handleNavigate}
        sidebarOpen={sidebarOpen}
        onToggleSidebar={() => setSidebarOpen(!sidebarOpen)}
      >
        {activePage === 'home' && (
          <HomePage
            onNavigate={handleNavigate}
            onChatWithPrompt={handleChatWithPrompt}
          />
        )}
        {activePage === 'chat' && (
          <ChatPage initialPrompt={chatPrompt} />
        )}
        {activePage === 'timeline' && (
          <div className="max-w-5xl mx-auto px-6 py-8">
            <Timeline />
          </div>
        )}
        {activePage === 'workflows' && (
          <div className="max-w-5xl mx-auto px-6 py-8">
            <PersonalDashboard />
          </div>
        )}
      </AppShell>

      {/* Settings Overlay */}
      <SettingsModal
        isOpen={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />
    </>
  );
}

export default App;
