import type { ChatSession } from '../../types';
import { getRelativeTime } from '../../lib/utils';
import {
    Plus,
    MessageCircle,
    Trash2,
} from 'lucide-react';

interface ChatSidebarProps {
    sessions: ChatSession[];
    activeSessionId: string | null;
    onSelectSession: (id: string) => void;
    onNewSession: () => void;
    onDeleteSession: (id: string) => void;
}

export function ChatSidebar({
    sessions,
    activeSessionId,
    onSelectSession,
    onNewSession,
    onDeleteSession,
}: ChatSidebarProps) {
    return (
        <div className="w-64 flex-shrink-0 bg-dark-900 border-r border-dark-700 flex flex-col h-full">
            {/* New Chat button */}
            <div className="p-3 border-b border-dark-800">
                <button
                    onClick={onNewSession}
                    className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-primary-600 hover:bg-primary-500 text-white rounded-lg text-sm font-medium transition-colors"
                >
                    <Plus className="w-4 h-4" />
                    New Chat
                </button>
            </div>

            {/* Session list */}
            <div className="flex-1 overflow-y-auto p-2 space-y-1">
                {sessions.length === 0 ? (
                    <div className="text-center py-8">
                        <MessageCircle className="w-8 h-8 text-dark-600 mx-auto mb-2" />
                        <p className="text-sm text-dark-500">No chats yet</p>
                        <p className="text-xs text-dark-600 mt-1">Start a new conversation</p>
                    </div>
                ) : (
                    sessions.map((session) => (
                        <div
                            key={session.id}
                            className={`group flex items-center gap-2 px-3 py-2.5 rounded-lg cursor-pointer transition-colors ${activeSessionId === session.id
                                    ? 'bg-dark-700 ring-1 ring-primary-600/30'
                                    : 'hover:bg-dark-800'
                                }`}
                            onClick={() => onSelectSession(session.id)}
                        >
                            <MessageCircle className={`w-4 h-4 flex-shrink-0 ${activeSessionId === session.id ? 'text-primary-400' : 'text-dark-500'
                                }`} />
                            <div className="flex-1 min-w-0">
                                <p className={`text-sm font-medium truncate ${activeSessionId === session.id ? 'text-white' : 'text-dark-300'
                                    }`}>
                                    {session.title}
                                </p>
                                <p className="text-[10px] text-dark-500">
                                    {getRelativeTime(session.updated_at)}
                                </p>
                            </div>
                            <button
                                onClick={(e) => {
                                    e.stopPropagation();
                                    onDeleteSession(session.id);
                                }}
                                className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-red-600/20 text-dark-500 hover:text-red-400 transition-all"
                            >
                                <Trash2 className="w-3.5 h-3.5" />
                            </button>
                        </div>
                    ))
                )}
            </div>
        </div>
    );
}
