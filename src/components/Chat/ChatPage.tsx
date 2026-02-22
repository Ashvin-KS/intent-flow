import { useState, useEffect, useRef } from 'react';
import type { ChatSession, ChatMessage as ChatMessageType } from '../../types';
import {
    createChatSession,
    getChatSessions,
    deleteChatSession,
    getChatMessages,
    sendChatMessage,
} from '../../services/tauri';
import { ChatMessage } from './ChatMessage';
import {
    Send,
    Loader2,
    Bot,
    Sparkles,
    ChevronDown,
    Grid3X3,
    Calendar,
    Check,
    Plus,
    Trash2,
    MessageSquare,
    PanelLeftClose,
    PanelLeft,
} from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { useFavoriteModels } from '../../hooks/useFavoriteModels';
import { useSettings } from '../../hooks/useSettings';

// Source options
const SOURCE_OPTIONS = [
    { id: 'apps', label: 'Applications', default: true },
    { id: 'screen', label: 'Screen Text (OCR)', default: true },
    { id: 'media', label: 'Media / Music', default: true },
    { id: 'browser', label: 'Browser History', default: false },
    { id: 'files', label: 'Files & Documents', default: false },
];

// Time range options
const TIME_RANGE_OPTIONS = [
    { id: 'today', label: 'Today' },
    { id: 'yesterday', label: 'Yesterday' },
    { id: 'last_3_days', label: 'Last 3 Days' },
    { id: 'last_7_days', label: 'Last 7 Days' },
    { id: 'last_30_days', label: 'Last 30 Days' },
    { id: 'this_year', label: 'This Year' },
    { id: 'all_time', label: 'All Time' },
];

interface ChatPageProps {
    initialPrompt?: string;
}

const CHAT_MODEL_STORAGE_KEY = 'intentflow_chat_selected_model';

interface ConfirmActionPayload {
    kind: string;
    reason: string;
    suggested_time_range?: string;
    enable_sources?: string[];
    retry_message: string;
}

interface ParsedAssistantAction {
    cleanedContent: string;
    action: ConfirmActionPayload | null;
}

function parseAssistantAction(content: string): ParsedAssistantAction {
    const marker = /\[\[IF_ACTION:(\{[\s\S]*\})\]\]/m;
    const match = content.match(marker);
    if (!match) {
        return { cleanedContent: content, action: null };
    }
    let action: ConfirmActionPayload | null = null;
    try {
        action = JSON.parse(match[1]) as ConfirmActionPayload;
    } catch {
        action = null;
    }
    const cleanedContent = content.replace(marker, '').trim();
    return { cleanedContent, action };
}

function loadSelectedModelFromStorage(): string {
    try {
        return localStorage.getItem(CHAT_MODEL_STORAGE_KEY) || '';
    } catch {
        return '';
    }
}

export function ChatPage({ initialPrompt }: ChatPageProps) {
    const [sessions, setSessions] = useState<ChatSession[]>([]);
    const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
    const [messages, setMessages] = useState<ChatMessageType[]>([]);
    const [input, setInput] = useState('');
    const [isSending, setIsSending] = useState(false);
    const [streamingContent, setStreamingContent] = useState('');
    const [agentStatus, setAgentStatus] = useState('');
    const [displayedStatus, setDisplayedStatus] = useState('');
    const [showHistory, setShowHistory] = useState(true);
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLTextAreaElement>(null);
    const initialPromptHandled = useRef(false);

    // Dropdown states
    const [showModelDropdown, setShowModelDropdown] = useState(false);
    const [showSourcesDropdown, setShowSourcesDropdown] = useState(false);
    const [showTimeDropdown, setShowTimeDropdown] = useState(false);
    const [selectedSources, setSelectedSources] = useState<string[]>(
        SOURCE_OPTIONS.filter((s) => s.default).map((s) => s.id)
    );
    const [selectedTimeRange, setSelectedTimeRange] = useState('today');
    const [selectedModel, setSelectedModel] = useState<string>(loadSelectedModelFromStorage);
    const [pendingAction, setPendingAction] = useState<ConfirmActionPayload | null>(null);

    // Hooks
    const { favorites, addFavorite } = useFavoriteModels();
    const { settings } = useSettings();

    // Refs for dropdowns
    const modelRef = useRef<HTMLDivElement>(null);
    const sourcesRef = useRef<HTMLDivElement>(null);
    const timeRef = useRef<HTMLDivElement>(null);

    // Sync selected model with Settings model updates only if user has no explicit chat selection.
    useEffect(() => {
        if (!settings?.ai.model) return;
        if (!selectedModel) {
            setSelectedModel(settings.ai.model);
        }
    }, [settings?.ai.model, selectedModel]);

    useEffect(() => {
        try {
            if (selectedModel) {
                localStorage.setItem(CHAT_MODEL_STORAGE_KEY, selectedModel);
            } else {
                localStorage.removeItem(CHAT_MODEL_STORAGE_KEY);
            }
        } catch {
            // ignore storage errors
        }
    }, [selectedModel]);

    // Close dropdowns on outside click
    useEffect(() => {
        const handler = (e: MouseEvent) => {
            if (modelRef.current && !modelRef.current.contains(e.target as Node)) {
                setShowModelDropdown(false);
            }
            if (sourcesRef.current && !sourcesRef.current.contains(e.target as Node)) {
                setShowSourcesDropdown(false);
            }
            if (timeRef.current && !timeRef.current.contains(e.target as Node)) {
                setShowTimeDropdown(false);
            }
        };
        document.addEventListener('mousedown', handler);
        return () => document.removeEventListener('mousedown', handler);
    }, []);

    // Load sessions on mount
    useEffect(() => {
        loadSessions();
    }, []);

    // Handle initial prompt (from Homepage summary cards)
    useEffect(() => {
        if (initialPrompt && !initialPromptHandled.current && !isSending) {
            initialPromptHandled.current = true;
            setInput(initialPrompt);
            // Auto-send after a short delay to let state settle
            setTimeout(() => {
                handleSendWithMessage(initialPrompt);
            }, 300);
        }
    }, [initialPrompt]);

    // Load messages when active session changes
    useEffect(() => {
        if (activeSessionId) {
            loadMessages(activeSessionId);
        } else {
            setMessages([]);
        }
        setStreamingContent('');
    }, [activeSessionId]);

    // Auto-scroll to bottom
    useEffect(() => {
        messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [messages, streamingContent]);

    // Listen for streaming tokens
    useEffect(() => {
        let unlistenToken: (() => void) | undefined;
        let unlistenStatus: (() => void) | undefined;
        let unlistenDone: (() => void) | undefined;
        async function setupListener() {
            unlistenToken = await listen<string>('chat://token', (event) => {
                setStreamingContent((prev) => prev + event.payload);
            });
            unlistenStatus = await listen<string>('chat://status', (event) => {
                setAgentStatus(event.payload || '');
            });
            unlistenDone = await listen<string>('chat://done', () => {
                setAgentStatus('');
                setDisplayedStatus('');
            });
        }
        setupListener();
        return () => {
            if (unlistenToken) unlistenToken();
            if (unlistenStatus) unlistenStatus();
            if (unlistenDone) unlistenDone();
        };
    }, []);

    useEffect(() => {
        if (!agentStatus) {
            setDisplayedStatus('');
            return;
        }
        let i = 0;
        const timer = window.setInterval(() => {
            i = Math.min(i + 1, agentStatus.length);
            setDisplayedStatus(agentStatus.slice(0, i));
            if (i >= agentStatus.length) {
                window.clearInterval(timer);
            }
        }, 12);
        return () => window.clearInterval(timer);
    }, [agentStatus]);

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
            setInput('');
            setStreamingContent('');
            inputRef.current?.focus();
        } catch (error) {
            console.error('Failed to create session:', error);
        }
    };

    const handleDeleteSession = async (id: string, e: React.MouseEvent) => {
        e.stopPropagation();
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

    const handleSendWithMessage = async (
        messageText: string,
        overrides?: { timeRange?: string; sources?: string[] }
    ) => {
        if (!messageText.trim() || isSending) return;

        let sessionId = activeSessionId;
        if (!sessionId) {
            try {
                const session = await createChatSession();
                setSessions((prev) => [session, ...prev]);
                setActiveSessionId(session.id);
                sessionId = session.id;
            } catch (error) {
                console.error('Failed to create session:', error);
                return;
            }
        }

        setInput('');
        setIsSending(true);
        setStreamingContent('');
        setAgentStatus('Preparing search...');

        const tempUserMsg: ChatMessageType = {
            id: Date.now(),
            session_id: sessionId,
            role: 'user',
            content: messageText.trim(),
            created_at: Math.floor(Date.now() / 1000),
        };
        setMessages((prev) => [...prev, tempUserMsg]);

        try {
            const response = await sendChatMessage(
                sessionId,
                messageText.trim(),
                selectedModel || undefined,
                overrides?.timeRange || selectedTimeRange,
                overrides?.sources || selectedSources
            );
            const { cleanedContent, action } = parseAssistantAction(response.content);
            const normalizedResponse: ChatMessageType = {
                ...response,
                content: cleanedContent || 'Please confirm the suggested scope/source update to continue.',
            };
            if (selectedModel) {
                const selected = favorites.find((f) => f.id === selectedModel);
                addFavorite({ id: selectedModel, name: selected?.name || selectedModel });
            }
            setMessages((prev) => [...prev, normalizedResponse]);
            if (action?.kind === 'confirm_scope_or_sources') {
                setPendingAction(action);
            }
            loadSessions(); // Refresh sessions to update titles
        } catch (error) {
            console.error('Failed to send message:', error);
            const errorMsg: ChatMessageType = {
                id: Date.now() + 1,
                session_id: sessionId,
                role: 'assistant',
                content: `Sorry, something went wrong: ${error}`,
                created_at: Math.floor(Date.now() / 1000),
            };
            setMessages((prev) => [...prev, errorMsg]);
        } finally {
            setIsSending(false);
            setStreamingContent('');
            setAgentStatus('');
            setDisplayedStatus('');
        }
    };

    const handleSend = () => handleSendWithMessage(input);

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            handleSend();
        }
    };

    const toggleSource = (sourceId: string) => {
        setSelectedSources((prev) =>
            prev.includes(sourceId)
                ? prev.filter((s) => s !== sourceId)
                : [...prev, sourceId]
        );
    };

    const renderStreamingMessage = () => {
        if (!streamingContent) return null;
        const normalized = streamingContent.trim();
        const looksLikeToolJson =
            normalized.startsWith('{') ||
            normalized.startsWith(', "reasoning"') ||
            streamingContent.includes('"tool"') ||
            streamingContent.includes('"args"') ||
            streamingContent.includes('"reasoning"') ||
            streamingContent.includes('<|tool_') ||
            streamingContent.includes('tool_call_');
        if (looksLikeToolJson) {
            const toolNameMatch = streamingContent.match(/"tool"\s*:\s*"([^"]+)"/);
            const reasoningMatch = streamingContent.match(/"reasoning"\s*:\s*"([\s\S]*?)"/);
            const toolName = toolNameMatch?.[1] || 'tool';
            const reasoningText = reasoningMatch?.[1]
                ?.replace(/\\"/g, '"')
                ?.replace(/\\n/g, '\n')
                ?.trim();
            return (
                <div className="flex justify-start mb-4 animate-pulse">
                    <div className="max-w-[85%] bg-dark-800 rounded-2xl rounded-bl-md px-4 py-3 border border-dark-700">
                        <div className="flex items-center gap-2 text-primary-400 mb-2">
                            <Bot className="w-4 h-4" />
                            <span className="text-sm font-medium">Thinking</span>
                        </div>
                        <div className="text-xs text-dark-300 space-y-1">
                            {reasoningText && <p className="whitespace-pre-wrap">{reasoningText}</p>}
                            <p className="text-dark-400">Running `{toolName}`...</p>
                        </div>
                    </div>
                </div>
            );
        }
        const tempMsg: ChatMessageType = {
            id: -1,
            session_id: activeSessionId || '',
            role: 'assistant',
            content: streamingContent,
            created_at: Date.now() / 1000,
        };
        return <ChatMessage message={tempMsg} isStreaming={true} />;
    };

    const getModelDisplayName = () => {
        if (!selectedModel) return 'Select Model';
        const fav = favorites.find((f) => f.id === selectedModel);
        if (fav) return fav.name;
        const parts = selectedModel.split('/');
        return parts[parts.length - 1] || selectedModel;
    };

    const getTimeRangeLabel = () =>
        TIME_RANGE_OPTIONS.find((t) => t.id === selectedTimeRange)?.label || 'Today';

    const getSourcesSummary = () => {
        if (selectedSources.length === SOURCE_OPTIONS.length) return 'All Sources';
        if (selectedSources.length === 0) return 'No Sources';
        return `${selectedSources.length} Sources`;
    };

    const getSourceLabelById = (id: string) =>
        SOURCE_OPTIONS.find((s) => s.id === id)?.label || id;

    const getTimeRangeLabelById = (id?: string) =>
        TIME_RANGE_OPTIONS.find((t) => t.id === id)?.label || id || '';

    const handleConfirmAction = async () => {
        if (!pendingAction) return;
        const nextTimeRange = pendingAction.suggested_time_range || selectedTimeRange;
        const nextSources = Array.from(
            new Set([...(selectedSources || []), ...(pendingAction.enable_sources || [])])
        );

        if (pendingAction.suggested_time_range) {
            setSelectedTimeRange(nextTimeRange);
        }
        if ((pendingAction.enable_sources || []).length > 0) {
            setSelectedSources(nextSources);
        }

        const retryMessage = pendingAction.retry_message || input.trim();
        setPendingAction(null);
        await handleSendWithMessage(retryMessage, {
            timeRange: nextTimeRange,
            sources: nextSources,
        });
    };

    const formatSessionDate = (timestamp: number) => {
        const d = new Date(timestamp * 1000);
        const now = new Date();
        const diffMs = now.getTime() - d.getTime();
        const diffMins = Math.floor(diffMs / 60000);
        if (diffMins < 1) return 'Just now';
        if (diffMins < 60) return `${diffMins}m ago`;
        const diffHours = Math.floor(diffMins / 60);
        if (diffHours < 24) return `${diffHours}h ago`;
        const diffDays = Math.floor(diffHours / 24);
        if (diffDays < 7) return `${diffDays}d ago`;
        return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
    };

    const hasMessages = messages.length > 0;

    // Control bar (shared between empty and message states)
    const controlBar = (
        <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-2 flex-wrap">
                {/* Model selector */}
                <div className="relative" ref={modelRef}>
                    <button
                        onClick={() => {
                            setShowModelDropdown(!showModelDropdown);
                            setShowSourcesDropdown(false);
                            setShowTimeDropdown(false);
                        }}
                        className="flex items-center gap-1.5 px-3 py-2 rounded-full bg-dark-800 border border-dark-700/50 text-sm text-dark-200 hover:text-white hover:border-dark-600 transition-colors"
                    >
                        <Sparkles className="w-3.5 h-3.5 text-blue-400" />
                        <span className="text-xs font-medium max-w-[120px] truncate">{getModelDisplayName()}</span>
                        <ChevronDown className={`w-3 h-3 text-dark-500 transition-transform ${showModelDropdown ? 'rotate-180' : ''}`} />
                    </button>
                    {showModelDropdown && (
                        <div className="absolute bottom-full mb-2 left-0 w-64 bg-dark-800 border border-dark-700 rounded-xl shadow-2xl shadow-black/40 z-50 py-1 max-h-64 overflow-y-auto">
                            <div className="px-3 py-2 border-b border-dark-700/50">
                                <p className="text-[10px] font-semibold text-dark-400 uppercase tracking-wider">Recent Models</p>
                            </div>
                            {favorites.length === 0 ? (
                                <div className="px-3 py-4 text-center">
                                    <p className="text-xs text-dark-500">No recent models</p>
                                    <p className="text-[10px] text-dark-600 mt-1">Select a model in Settings or send a chat to populate recents</p>
                                </div>
                            ) : (
                                favorites.map((model) => (
                                    <button
                                        key={model.id}
                                        onClick={() => { setSelectedModel(model.id); addFavorite({ id: model.id, name: model.name }); setShowModelDropdown(false); }}
                                        className={`w-full flex items-center gap-2 px-3 py-2 text-left text-sm hover:bg-dark-700/50 transition-colors ${selectedModel === model.id ? 'text-blue-400' : 'text-dark-200'
                                            }`}
                                    >
                                        <Sparkles className="w-3.5 h-3.5 flex-shrink-0" />
                                        <span className="flex-1 truncate text-xs">{model.name}</span>
                                        {selectedModel === model.id && <Check className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />}
                                    </button>
                                ))
                            )}
                            {settings?.ai.model && !favorites.some((f) => f.id === settings.ai.model) && (
                                <>
                                    <div className="px-3 py-2 border-t border-dark-700/50">
                                        <p className="text-[10px] font-semibold text-dark-400 uppercase tracking-wider">Current (Settings)</p>
                                    </div>
                                    <button
                                        onClick={() => { setSelectedModel(settings.ai.model); setShowModelDropdown(false); }}
                                        className={`w-full flex items-center gap-2 px-3 py-2 text-left text-sm hover:bg-dark-700/50 transition-colors ${selectedModel === settings.ai.model ? 'text-blue-400' : 'text-dark-200'
                                            }`}
                                    >
                                        <Sparkles className="w-3.5 h-3.5 flex-shrink-0" />
                                        <span className="flex-1 truncate text-xs">{settings.ai.model}</span>
                                        {selectedModel === settings.ai.model && <Check className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />}
                                    </button>
                                </>
                            )}
                        </div>
                    )}
                </div>

                {/* Sources */}
                <div className="relative" ref={sourcesRef}>
                    <button
                        onClick={() => {
                            setShowSourcesDropdown(!showSourcesDropdown);
                            setShowModelDropdown(false);
                            setShowTimeDropdown(false);
                        }}
                        className="flex items-center gap-1.5 px-3 py-2 rounded-full bg-dark-800 border border-dark-700/50 text-sm text-dark-200 hover:text-white hover:border-dark-600 transition-colors"
                    >
                        <Grid3X3 className="w-3.5 h-3.5" />
                        <span className="text-xs font-medium">{getSourcesSummary()}</span>
                        <ChevronDown className={`w-3 h-3 text-dark-500 transition-transform ${showSourcesDropdown ? 'rotate-180' : ''}`} />
                    </button>
                    {showSourcesDropdown && (
                        <div className="absolute bottom-full mb-2 left-0 w-56 bg-dark-800 border border-dark-700 rounded-xl shadow-2xl shadow-black/40 z-50 py-1">
                            <div className="px-3 py-2 border-b border-dark-700/50">
                                <p className="text-[10px] font-semibold text-dark-400 uppercase tracking-wider">Data Sources</p>
                            </div>
                            {SOURCE_OPTIONS.map((source) => (
                                <button
                                    key={source.id}
                                    onClick={() => toggleSource(source.id)}
                                    className="w-full flex items-center gap-2.5 px-3 py-2 text-left text-sm hover:bg-dark-700/50 transition-colors"
                                >
                                    <div className={`w-4 h-4 rounded border flex items-center justify-center flex-shrink-0 transition-colors ${selectedSources.includes(source.id) ? 'bg-blue-500 border-blue-500' : 'border-dark-600 bg-transparent'
                                        }`}>
                                        {selectedSources.includes(source.id) && <Check className="w-3 h-3 text-white" />}
                                    </div>
                                    <span className={`text-xs ${selectedSources.includes(source.id) ? 'text-white' : 'text-dark-300'}`}>
                                        {source.label}
                                    </span>
                                </button>
                            ))}
                        </div>
                    )}
                </div>

                {/* Time Range */}
                <div className="relative" ref={timeRef}>
                    <button
                        onClick={() => {
                            setShowTimeDropdown(!showTimeDropdown);
                            setShowModelDropdown(false);
                            setShowSourcesDropdown(false);
                        }}
                        className="flex items-center gap-1.5 px-3 py-2 rounded-full bg-dark-800 border border-dark-700/50 text-sm text-dark-200 hover:text-white hover:border-dark-600 transition-colors"
                    >
                        <Calendar className="w-3.5 h-3.5" />
                        <span className="text-xs font-medium">{getTimeRangeLabel()}</span>
                        <ChevronDown className={`w-3 h-3 text-dark-500 transition-transform ${showTimeDropdown ? 'rotate-180' : ''}`} />
                    </button>
                    {showTimeDropdown && (
                        <div className="absolute bottom-full mb-2 left-0 w-48 bg-dark-800 border border-dark-700 rounded-xl shadow-2xl shadow-black/40 z-50 py-1">
                            <div className="px-3 py-2 border-b border-dark-700/50">
                                <p className="text-[10px] font-semibold text-dark-400 uppercase tracking-wider">Time Range</p>
                            </div>
                            {TIME_RANGE_OPTIONS.map((range) => (
                                <button
                                    key={range.id}
                                    onClick={() => { setSelectedTimeRange(range.id); setShowTimeDropdown(false); }}
                                    className={`w-full flex items-center gap-2 px-3 py-2 text-left text-sm hover:bg-dark-700/50 transition-colors ${selectedTimeRange === range.id ? 'text-blue-400' : 'text-dark-200'
                                        }`}
                                >
                                    <span className="flex-1 text-xs">{range.label}</span>
                                    {selectedTimeRange === range.id && <Check className="w-3.5 h-3.5 text-blue-400" />}
                                </button>
                            ))}
                        </div>
                    )}
                </div>
            </div>

            {/* Send */}
            <button
                onClick={handleSend}
                disabled={!input.trim() || isSending}
                className="flex items-center gap-1.5 px-4 py-2 rounded-full bg-dark-800 border border-dark-700/50 text-dark-200 hover:text-white hover:border-dark-600 disabled:opacity-30 disabled:hover:text-dark-200 transition-colors"
                id="chat-send"
            >
                {isSending ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Send className="w-3.5 h-3.5" />}
                <span className="text-xs font-medium">Send</span>
            </button>
        </div>
    );

    return (
        <div className="flex h-full bg-dark-950">
            {/* Chat History Panel */}
            {showHistory && (
                <div className="w-56 flex-shrink-0 bg-dark-900/60 border-r border-dark-800/50 flex flex-col">
                    {/* History Header */}
                    <div className="flex items-center justify-between px-3 py-3 border-b border-dark-800/50">
                        <span className="text-xs font-semibold text-dark-400 uppercase tracking-wider">History</span>
                        <div className="flex items-center gap-1">
                            <button
                                onClick={handleNewSession}
                                className="w-7 h-7 flex items-center justify-center rounded-md text-dark-400 hover:text-white hover:bg-dark-800 transition-colors"
                                title="New chat"
                            >
                                <Plus className="w-4 h-4" />
                            </button>
                            <button
                                onClick={() => setShowHistory(false)}
                                className="w-7 h-7 flex items-center justify-center rounded-md text-dark-400 hover:text-white hover:bg-dark-800 transition-colors"
                                title="Close history"
                            >
                                <PanelLeftClose className="w-4 h-4" />
                            </button>
                        </div>
                    </div>

                    {/* Session List */}
                    <div className="flex-1 overflow-y-auto py-1">
                        {sessions.length === 0 ? (
                            <div className="px-3 py-8 text-center">
                                <MessageSquare className="w-8 h-8 text-dark-700 mx-auto mb-2" />
                                <p className="text-xs text-dark-500">No conversations yet</p>
                                <button
                                    onClick={handleNewSession}
                                    className="mt-3 text-xs text-primary-400 hover:text-primary-300 transition-colors"
                                >
                                    Start your first chat
                                </button>
                            </div>
                        ) : (
                            sessions.map((session) => (
                                <button
                                    key={session.id}
                                    onClick={() => setActiveSessionId(session.id)}
                                    className={`group w-full flex items-start gap-2 px-3 py-2.5 text-left transition-colors ${activeSessionId === session.id
                                            ? 'bg-dark-800/80 text-white'
                                            : 'text-dark-300 hover:bg-dark-800/40 hover:text-dark-200'
                                        }`}
                                >
                                    <MessageSquare className="w-3.5 h-3.5 mt-0.5 flex-shrink-0 text-dark-500" />
                                    <div className="flex-1 min-w-0">
                                        <p className="text-xs font-medium truncate">
                                            {session.title || 'New Chat'}
                                        </p>
                                        <p className="text-[10px] text-dark-500 mt-0.5">
                                            {formatSessionDate(session.updated_at)}
                                        </p>
                                    </div>
                                    <button
                                        onClick={(e) => handleDeleteSession(session.id, e)}
                                        className="opacity-0 group-hover:opacity-100 flex-shrink-0 w-6 h-6 flex items-center justify-center rounded text-dark-500 hover:text-red-400 hover:bg-dark-700/50 transition-all"
                                        title="Delete"
                                    >
                                        <Trash2 className="w-3 h-3" />
                                    </button>
                                </button>
                            ))
                        )}
                    </div>
                </div>
            )}

            {/* Main Chat Area */}
            <div className="flex-1 flex flex-col min-w-0">
                {/* Top: History toggle if hidden */}
                {!showHistory && (
                    <div className="px-4 py-2 flex-shrink-0">
                        <button
                            onClick={() => setShowHistory(true)}
                            className="w-8 h-8 flex items-center justify-center rounded-lg text-dark-400 hover:text-white hover:bg-dark-800 transition-colors"
                            title="Show chat history"
                        >
                            <PanelLeft className="w-[18px] h-[18px]" />
                        </button>
                    </div>
                )}

                {hasMessages ? (
                    <>
                        {/* Messages */}
                        <div className="flex-1 overflow-y-auto px-6 py-4">
                            <div className="max-w-3xl mx-auto">
                                {messages.map((msg) => (
                                    <ChatMessage key={msg.id} message={msg} />
                                ))}
                                {streamingContent ? renderStreamingMessage() : isSending && (
                                    <div className="flex items-center gap-2 text-dark-400 mb-4">
                                        <div className="bg-dark-800 rounded-2xl rounded-bl-md px-4 py-3 border border-dark-700">
                                            <div className="flex items-center gap-2">
                                                <Loader2 className="w-4 h-4 animate-spin" />
                                                <span className="text-sm">
                                                    {displayedStatus || agentStatus || 'Thinking...'}
                                                    <span className="inline-flex ml-1">
                                                        <span className="animate-pulse">.</span>
                                                        <span className="animate-pulse [animation-delay:120ms]">.</span>
                                                        <span className="animate-pulse [animation-delay:240ms]">.</span>
                                                    </span>
                                                </span>
                                            </div>
                                        </div>
                                    </div>
                                )}
                                {isSending && agentStatus && streamingContent && (
                                    <div className="flex items-center gap-2 text-dark-500 mb-4">
                                        <div className="bg-dark-900/70 rounded-xl px-3 py-2 border border-dark-800">
                                            <span className="text-xs">
                                                {displayedStatus || agentStatus}
                                            </span>
                                        </div>
                                    </div>
                                )}
                                <div ref={messagesEndRef} />
                            </div>
                        </div>

                        {/* Input (with messages) */}
                        <div className="border-t border-dark-800/50 bg-dark-950 px-6 py-4">
                            <div className="max-w-3xl mx-auto space-y-3">
                                <textarea
                                    ref={inputRef}
                                    value={input}
                                    onChange={(e) => setInput(e.target.value)}
                                    onKeyDown={handleKeyDown}
                                    placeholder="Ask about your activity..."
                                    rows={1}
                                    className="w-full resize-none bg-dark-800/60 border border-dark-700/50 text-white rounded-xl px-4 py-3 text-sm placeholder-dark-500 focus:outline-none focus:ring-1 focus:ring-dark-600 focus:border-dark-600"
                                    style={{ minHeight: '44px', maxHeight: '120px' }}
                                    onInput={(e) => {
                                        const t = e.target as HTMLTextAreaElement;
                                        t.style.height = 'auto';
                                        t.style.height = `${Math.min(t.scrollHeight, 120)}px`;
                                    }}
                                />
                                {controlBar}
                            </div>
                        </div>
                    </>
                ) : (
                    /* Empty state */
                    <div className="flex-1 flex flex-col items-center justify-center px-6">
                        <div className="w-full max-w-2xl space-y-4">
                            <textarea
                                ref={inputRef}
                                value={input}
                                onChange={(e) => setInput(e.target.value)}
                                onKeyDown={handleKeyDown}
                                placeholder="Ask about moments or topics from your memories..."
                                rows={1}
                                className="w-full resize-none bg-dark-800/40 border border-dark-700/40 text-white rounded-2xl px-5 py-4 text-sm placeholder-dark-500 focus:outline-none focus:ring-1 focus:ring-dark-600 focus:border-dark-600"
                                style={{ minHeight: '52px', maxHeight: '120px' }}
                                onInput={(e) => {
                                    const t = e.target as HTMLTextAreaElement;
                                    t.style.height = 'auto';
                                    t.style.height = `${Math.min(t.scrollHeight, 120)}px`;
                                }}
                            />
                            {controlBar}
                        </div>
                    </div>
                )}
            </div>

            {pendingAction && (
                <div className="fixed inset-0 z-[80] flex items-center justify-center bg-black/55 px-4">
                    <div className="w-full max-w-md rounded-2xl border border-dark-700 bg-dark-900 p-5 shadow-2xl">
                        <h3 className="text-sm font-semibold text-white">Allow Scope Update?</h3>
                        <p className="mt-2 text-xs text-dark-300 leading-relaxed">{pendingAction.reason}</p>
                        {pendingAction.suggested_time_range && (
                            <p className="mt-2 text-xs text-dark-200">
                                Time range change:
                                <span className="text-blue-400"> {getTimeRangeLabelById(selectedTimeRange)}</span>
                                {' -> '}
                                <span className="text-blue-400">{getTimeRangeLabelById(pendingAction.suggested_time_range)}</span>
                            </p>
                        )}
                        {(pendingAction.enable_sources || []).length > 0 && (
                            <p className="mt-2 text-xs text-dark-200">
                                Enable sources:
                                <span className="text-blue-400">
                                    {' '}
                                    {(pendingAction.enable_sources || []).map(getSourceLabelById).join(', ')}
                                </span>
                            </p>
                        )}
                        <div className="mt-4 flex items-center justify-end gap-2">
                            <button
                                onClick={() => setPendingAction(null)}
                                className="px-3 py-1.5 rounded-lg border border-dark-700 text-xs text-dark-300 hover:text-white hover:border-dark-600 transition-colors"
                            >
                                Cancel
                            </button>
                            <button
                                onClick={handleConfirmAction}
                                className="px-3 py-1.5 rounded-lg bg-blue-600 text-xs text-white hover:bg-blue-500 transition-colors"
                            >
                                Yes, Continue
                            </button>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}


