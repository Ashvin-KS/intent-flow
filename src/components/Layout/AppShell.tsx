import { type ReactNode } from 'react';
import type { PageType } from '../../App';
import {
    PanelLeft,
    Home,
    Crown,
    Settings,
} from 'lucide-react';
import { Sidebar } from './Sidebar';

interface AppShellProps {
    children: ReactNode;
    activePage: PageType;
    onNavigate: (page: PageType) => void;
    sidebarOpen: boolean;
    onToggleSidebar: () => void;
}

export function AppShell({
    children,
    activePage,
    onNavigate,
    sidebarOpen,
    onToggleSidebar,
}: AppShellProps) {
    return (
        <div className="h-screen overflow-hidden bg-dark-950 flex flex-col">
            {/* Top Bar */}
            <header
                className="h-14 flex-shrink-0 flex items-center px-4 gap-3 border-b border-dark-800/50"
                data-tauri-drag-region
            >
                {/* Left controls */}
                <div className="flex items-center gap-1">
                    <button
                        onClick={onToggleSidebar}
                        className="w-8 h-8 flex items-center justify-center rounded-lg text-dark-400 hover:text-white hover:bg-dark-800 transition-colors"
                        id="sidebar-toggle"
                        title="Toggle sidebar"
                    >
                        <PanelLeft className="w-[18px] h-[18px]" />
                    </button>
                    <button
                        onClick={() => onNavigate('home')}
                        className={`w-8 h-8 flex items-center justify-center rounded-lg transition-colors ${activePage === 'home'
                            ? 'text-white bg-dark-800'
                            : 'text-dark-400 hover:text-white hover:bg-dark-800'
                            }`}
                        id="home-button"
                        title="Home"
                    >
                        <Home className="w-[18px] h-[18px]" />
                    </button>
                </div>

                {/* User Profile */}
                <div className="flex items-center gap-2.5 px-3 py-1.5 rounded-xl bg-dark-800/50 border border-dark-700/30">
                    <div className="w-7 h-7 rounded-full bg-gradient-to-br from-orange-500 to-red-600 flex items-center justify-center text-white text-xs font-bold shadow-md shadow-orange-500/20">
                        U
                    </div>
                    <div className="leading-tight">
                        <p className="text-[13px] font-medium text-white">User</p>
                        <p className="text-[10px] text-dark-400 flex items-center gap-1">
                            <span className="w-1.5 h-1.5 rounded-full bg-dark-500 inline-block" />
                            Free
                        </p>
                    </div>
                </div>

                {/* Upgrade Button */}
                <button
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-gradient-to-r from-amber-500 to-orange-500 hover:from-amber-400 hover:to-orange-400 text-white text-xs font-semibold transition-all shadow-lg shadow-orange-500/25 hover:shadow-orange-500/40"
                    id="upgrade-button"
                >
                    <Crown className="w-3.5 h-3.5" />
                    Upgrade
                </button>

                {/* Spacer for drag region */}
                <div className="flex-1" data-tauri-drag-region />

                {/* Right: Settings */}
                <button
                    onClick={() => onNavigate('settings')}
                    className={`w-8 h-8 flex items-center justify-center rounded-lg transition-colors ${activePage === 'settings'
                        ? 'text-white bg-dark-800'
                        : 'text-dark-400 hover:text-white hover:bg-dark-800'
                        }`}
                    id="settings-button"
                    title="Settings"
                >
                    <Settings className="w-[18px] h-[18px]" />
                </button>
            </header>

            {/* Content Area */}
            <div className="flex-1 flex overflow-hidden">
                {/* Sidebar */}
                <Sidebar
                    isOpen={sidebarOpen}
                    activePage={activePage}
                    onNavigate={onNavigate}
                    onClose={() => onToggleSidebar()}
                />

                {/* Main Content */}
                <main className="flex-1 overflow-y-auto overflow-x-hidden">
                    {children}
                </main>
            </div>
        </div>
    );
}
