import type { PageType } from '../../App';
import {
    Home,
    MessageCircle,
    History,
    Zap,
    X,
} from 'lucide-react';

interface SidebarProps {
    isOpen: boolean;
    activePage: PageType;
    onNavigate: (page: PageType) => void;
    onClose: () => void;
}

const navItems: { id: PageType; label: string; icon: typeof Home }[] = [
    { id: 'home', label: 'Home', icon: Home },
    { id: 'chat', label: 'Chat', icon: MessageCircle },
    { id: 'timeline', label: 'Timeline', icon: History },
    { id: 'workflows', label: 'Dashboard', icon: Zap },
];

export function Sidebar({ isOpen, activePage, onNavigate, onClose }: SidebarProps) {
    if (!isOpen) return null;

    return (
        <>
            {/* Overlay (mobile only) */}
            <div
                className="fixed inset-0 bg-black/40 z-30 lg:hidden"
                onClick={onClose}
            />

            {/* Sidebar — part of the flex layout, not fixed */}
            <aside
                className="w-52 flex-shrink-0 bg-dark-900/80 border-r border-dark-800/50 flex flex-col
                    max-lg:fixed max-lg:z-40 max-lg:top-14 max-lg:bottom-0 max-lg:left-0"
            >
                {/* Close button (mobile) */}
                <div className="flex items-center justify-between px-4 py-3 lg:hidden">
                    <span className="text-[10px] font-semibold text-dark-500 uppercase tracking-wider">Menu</span>
                    <button
                        onClick={onClose}
                        className="text-dark-400 hover:text-white transition-colors"
                    >
                        <X className="w-4 h-4" />
                    </button>
                </div>

                {/* Navigation */}
                <nav className="flex-1 px-3 py-3 space-y-0.5">
                    {navItems.map((item) => (
                        <button
                            key={item.id}
                            onClick={() => {
                                onNavigate(item.id);
                                if (window.innerWidth < 1024) onClose();
                            }}
                            className={`
                                w-full flex items-center gap-3 px-3 py-2 rounded-lg
                                text-[13px] font-medium transition-all duration-100
                                ${activePage === item.id
                                    ? 'bg-dark-800 text-white'
                                    : 'text-dark-400 hover:text-dark-200 hover:bg-dark-800/40'
                                }
                            `}
                            id={`nav-${item.id}`}
                        >
                            <item.icon className="w-[18px] h-[18px]" />
                            {item.label}
                        </button>
                    ))}
                </nav>

                {/* Bottom */}
                <div className="px-4 py-3 border-t border-dark-800/50">
                    <p className="text-[10px] text-dark-600 text-center">IntentFlow v1.0</p>
                </div>
            </aside>
        </>
    );
}
