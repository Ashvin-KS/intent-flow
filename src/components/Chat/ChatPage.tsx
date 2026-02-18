import { useState, useEffect, useRef } from 'react';
import type { ChatSession, ChatMessage as ChatMessageType } from '../../types';
import {
    createChatSession,
    getChatSessions,
    deleteChatSession,
    getChatMessages,
    sendChatMessage,
} from '../../services/tauri';
import { ChatSidebar } from './ChatSidebar';
import { ChatMessage } from './ChatMessage';
import { Send, Loader2, Bot, Sparkles } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';

export function ChatPage() {
    const [sessions, setSessions] = useState<ChatSession[]>([]);
    const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
    const [messages, setMessages] = useState<ChatMessageType[]>([]);
    const [input, setInput] = useState('');
    const [isSending, setIsSending] = useState(false);
    const [streamingContent, setStreamingContent] = useState('');
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLTextAreaElement>(null);

    // Load sessions on mount
    useEffect(() => {
        loadSessions();
    }, []);

    // Load messages when active session changes
    useEffect(() => {
        if (activeSessionId) {
            loadMessages(activeSessionId);
        } else {
            setMessages([]);
        }
        setStreamingContent('');
    }, [activeSessionId]);

    // Auto-scroll to bottom on new messages or streaming update
    useEffect(() => {
        messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [messages, streamingContent]);

    // Listen for streaming tokens
    useEffect(() => {
        let unlisten: (() => void) | undefined;

        async function setupListener() {
            unlisten = await listen<string>('chat://token', (event) => {
                setStreamingContent((prev) => prev + event.payload);
            });
        }

        setupListener();

        return () => {
            if (unlisten) unlisten();
        };
    }, []);

    const loadSessions = async () => {
        try {
            const data = await getChatSessions();
            setSessions(data);
        } catch (error) {
            console.error('Failed to load sessions:', error);
        }
    };

    const loadMessages = async (sessionId: string) => {
        try {
            const data = await getChatMessages(sessionId);
            setMessages(data);
        } catch (error) {
            console.error('Failed to load messages:', error);
        }
    };

    const handleNewSession = async () => {
        try {
            const session = await createChatSession();
            setSessions((prev) => [session, ...prev]);
            setActiveSessionId(session.id);
            setMessages([]);

            // Clean state
            setInput('');
            setStreamingContent('');
            inputRef.current?.focus();
        } catch (error) {
            console.error('Failed to create session:', error);
        }
    };

    const handleDeleteSession = async (id: string) => {
        try {
            await deleteChatSession(id);
            setSessions((prev) => prev.filter((s) => s.id !== id));
            if (activeSessionId === id) {
                setActiveSessionId(null);
                setMessages([]);
            }
        } catch (error) {
            console.error('Failed to delete session:', error);
        }
    };

    const handleSend = async () => {
        if (!input.trim() || !activeSessionId || isSending) return;

        const userMessage = input.trim();
        setInput('');
        setIsSending(true);
        setStreamingContent('');

        // Optimistically add user message
        const tempUserMsg: ChatMessageType = {
            id: Date.now(),
            session_id: activeSessionId,
            role: 'user',
            content: userMessage,
            created_at: Math.floor(Date.now() / 1000),
        };
        setMessages((prev) => [...prev, tempUserMsg]);

        try {
            const response = await sendChatMessage(activeSessionId, userMessage);
            setMessages((prev) => [...prev, response]);
            // Refresh sessions (title may have changed)
            loadSessions();
        } catch (error) {
            console.error('Failed to send message:', error);
            const errorMsg: ChatMessageType = {
                id: Date.now() + 1,
                session_id: activeSessionId,
                role: 'assistant',
                content: `Sorry, something went wrong: ${error}`,
                created_at: Math.floor(Date.now() / 1000),
            };
            setMessages((prev) => [...prev, errorMsg]);
        } finally {
            setIsSending(false);
            setStreamingContent('');
        }
    };

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            handleSend();
        }
    };

    // Helper to render streaming content safely
    const renderStreamingMessage = () => {
        if (!streamingContent) return null;

        // If it starts with JSON brace, show "Thinking..."
        if (streamingContent.trim().startsWith('{')) {
            return (
                <div className="flex justify-start mb-4 animate-pulse">
                    <div className="max-w-[85%] bg-dark-800 rounded-2xl rounded-bl-md px-4 py-3 border border-dark-700">
                        <div className="flex items-center gap-2 text-primary-400">
                            <Bot className="w-4 h-4" />
                            <span className="text-sm font-medium">Thinking...</span>
                        </div>
                        {/* Optional: Show raw output for debug */}
                        {/* <pre className="text-[10px] text-dark-500 mt-2 overflow-hidden">{streamingContent.slice(-100)}</pre> */}
                    </div>
                </div>
            );
        }

        // Otherwise show text
        const tempMsg: ChatMessageType = {
            id: -1,
            session_id: activeSessionId || '',
            role: 'assistant',
            content: streamingContent,
            created_at: Date.now() / 1000,
        };

        return <ChatMessage message={tempMsg} />;
    };

    return (
        <div className="flex h-[calc(100vh-64px)] bg-dark-950">
            {/* Sidebar */}
            <ChatSidebar
                sessions={sessions}
                activeSessionId={activeSessionId}
                onSelectSession={setActiveSessionId}
                onNewSession={handleNewSession}
                onDeleteSession={handleDeleteSession}
            />

            {/* Chat Area */}
            <div className="flex-1 flex flex-col min-w-0">
                {activeSessionId ? (
                    <>
                        {/* Messages */}
                        <div className="flex-1 overflow-y-auto px-6 py-4">
                            {messages.length === 0 ? (
                                <div className="flex flex-col items-center justify-center h-full text-center">
                                    <div className="w-16 h-16 bg-primary-600/10 rounded-2xl flex items-center justify-center mb-4">
                                        <Sparkles className="w-8 h-8 text-primary-400" />
                                    </div>
                                    <h3 className="text-lg font-semibold text-white mb-1">Start a conversation</h3>
                                    <p className="text-sm text-dark-400 max-w-md">
                                        Ask about your activity history, screen content, music playback, or anything IntentFlow tracks.
                                    </p>
                                    <div className="mt-6 flex flex-wrap gap-2 justify-center">
                                        {[
                                            'What did I do today?',
                                            'What songs did I listen to?',
                                            'How much time on VS Code?',
                                            'What was on my screen earlier?',
                                        ].map((suggestion) => (
                                            <button
                                                key={suggestion}
                                                onClick={() => {
                                                    setInput(suggestion);
                                                    inputRef.current?.focus();
                                                }}
                                                className="px-3 py-1.5 text-xs bg-dark-800 border border-dark-700 text-dark-300 rounded-full hover:bg-dark-700 hover:text-white transition-colors"
                                            >
                                                {suggestion}
                                            </button>
                                        ))}
                                    </div>
                                </div>
                            ) : (
                                <>
                                    {messages.map((msg) => (
                                        <ChatMessage key={msg.id} message={msg} />
                                    ))}

                                    {/* Streaming Message or Loading State */}
                                    {streamingContent ? renderStreamingMessage() : isSending && (
                                        <div className="flex items-center gap-2 text-dark-400 mb-4 px-4">
                                            <div className="bg-dark-800 rounded-2xl rounded-bl-md px-4 py-3 border border-dark-700">
                                                <div className="flex items-center gap-2">
                                                    <Loader2 className="w-4 h-4 animate-spin" />
                                                    <span className="text-sm">Connecting...</span>
                                                </div>
                                            </div>
                                        </div>
                                    )}
                                    <div ref={messagesEndRef} />
                                </>
                            )}
                        </div>

                        {/* Input */}
                        <div className="border-t border-dark-800 bg-dark-900/50 backdrop-blur-sm px-6 py-4">
                            <div className="flex items-end gap-3 max-w-3xl mx-auto">
                                <div className="flex-1 relative">
                                    <textarea
                                        ref={inputRef}
                                        value={input}
                                        onChange={(e) => setInput(e.target.value)}
                                        onKeyDown={handleKeyDown}
                                        placeholder="Ask about your activity..."
                                        rows={1}
                                        className="w-full resize-none bg-dark-800 border border-dark-700 text-white rounded-xl px-4 py-3 text-sm placeholder-dark-500 focus:outline-none focus:ring-2 focus:ring-primary-600/50 focus:border-primary-600 transition-all"
                                        style={{
                                            minHeight: '44px',
                                            maxHeight: '120px',
                                        }}
                                        onInput={(e) => {
                                            const target = e.target as HTMLTextAreaElement;
                                            target.style.height = 'auto';
                                            target.style.height = `${Math.min(target.scrollHeight, 120)}px`;
                                        }}
                                    />
                                </div>
                                <button
                                    onClick={handleSend}
                                    disabled={!input.trim() || isSending}
                                    className="flex-shrink-0 p-3 bg-primary-600 hover:bg-primary-500 disabled:bg-dark-700 disabled:text-dark-500 text-white rounded-xl transition-colors"
                                >
                                    {isSending ? (
                                        <Loader2 className="w-5 h-5 animate-spin" />
                                    ) : (
                                        <Send className="w-5 h-5" />
                                    )}
                                </button>
                            </div>
                        </div>
                    </>
                ) : (
                    /* No session selected */
                    <div className="flex-1 flex flex-col items-center justify-center">
                        <div className="w-20 h-20 bg-dark-800 rounded-2xl flex items-center justify-center mb-4">
                            <Bot className="w-10 h-10 text-dark-500" />
                        </div>
                        <h2 className="text-xl font-semibold text-white mb-2">IntentFlow Chat</h2>
                        <p className="text-sm text-dark-400 mb-6 max-w-sm text-center">
                            Chat with an AI agent that queries your activity history, screen text, and media playback in real time.
                        </p>
                        <button
                            onClick={handleNewSession}
                            className="px-6 py-2.5 bg-primary-600 hover:bg-primary-500 text-white rounded-lg text-sm font-medium transition-colors"
                        >
                            Start a new chat
                        </button>
                    </div>
                )}
            </div>
        </div>
    );
}
