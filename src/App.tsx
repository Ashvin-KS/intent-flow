import { useState } from 'react';
import { HomePage } from './components/Home/HomePage';
import { ChatPage } from './components/Chat/ChatPage';
import { Timeline } from './components/Timeline/Timeline';
import { WorkflowList } from './components/Workflows/WorkflowList';
import { SettingsModal } from './components/Settings/SettingsModal';
import { AppShell } from './components/Layout/AppShell';

export type PageType = 'home' | 'chat' | 'timeline' | 'workflows' | 'settings';

function App() {
  const [activePage, setActivePage] = useState<PageType>('home');
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
            <WorkflowList />
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
