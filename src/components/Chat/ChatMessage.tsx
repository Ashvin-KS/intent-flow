import { useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import type { ChatMessage as ChatMessageType, AgentStep, ActivityRef } from '../../types';
import { formatTime } from '../../lib/utils';
import {
    ChevronDown,
    ChevronRight,
    Wrench,
    Music,
    Monitor,
    Clock,
    Brain,
} from 'lucide-react';

interface ChatMessageProps {
    message: ChatMessageType;
    isStreaming?: boolean;
}

export function ChatMessage({ message, isStreaming = false }: ChatMessageProps) {
    const [showSteps, setShowSteps] = useState(false);
    const [showThinking, setShowThinking] = useState(false);
    const [displayedText, setDisplayedText] = useState('');
    const isUser = message.role === 'user';
    const hasSteps = message.tool_calls && message.tool_calls.length > 0;
    const hasActivities = message.activities && message.activities.length > 0;
    const { answerText, thinkingText } = splitThinkingContent(message.content);
    const hasThinking = !isUser && thinkingText.length > 0;
    const bubbleTextRaw = isUser ? message.content : answerText || (hasThinking ? 'Thinking...' : message.content);
    const bubbleText = isUser ? bubbleTextRaw : stripToolJsonPayloads(bubbleTextRaw).trim();

    useEffect(() => {
        if (!isStreaming || isUser) {
            setDisplayedText(bubbleText);
            return;
        }

        const timer = window.setInterval(() => {
            setDisplayedText((prev) => {
                if (!bubbleText.startsWith(prev)) {
                    return bubbleText;
                }
                if (prev.length >= bubbleText.length) {
                    return prev;
                }
                return bubbleText.slice(0, prev.length + 1);
            });
        }, 8);

        return () => window.clearInterval(timer);
    }, [bubbleText, isStreaming, isUser]);

    return (
        <div className={`flex ${isUser ? 'justify-end' : 'justify-start'} mb-4`}>
            <div className={`max-w-[85%] ${isUser ? 'order-1' : 'order-1'}`}>
                {/* Thinking toggle */}
                {hasThinking && (
                    <div className="mt-2">
                        <button
                            onClick={() => setShowThinking(!showThinking)}
                            className="flex items-center gap-1.5 text-xs text-dark-400 hover:text-dark-200 transition-colors"
                        >
                            {showThinking ? (
                                <ChevronDown className="w-3 h-3" />
                            ) : (
                                <ChevronRight className="w-3 h-3" />
                            )}
                            <Brain className="w-3 h-3" />
                            <span>Thinking</span>
                        </button>

                        {showThinking && (
                            <div className="mt-2 bg-dark-900 border border-dark-700 rounded-lg p-3">
                                <p className="text-xs text-dark-300 whitespace-pre-wrap leading-relaxed">
                                    {thinkingText}
                                </p>
                            </div>
                        )}
                    </div>
                )}

                {/* Message bubble */}
                <div
                    className={`rounded-2xl px-4 py-3 mt-2 ${isUser
                        ? 'bg-primary-600 text-white rounded-br-md'
                        : 'bg-dark-800 text-dark-100 rounded-bl-md border border-dark-700'
                        }`}
                >
                    <MarkdownMessage text={isStreaming && !isUser ? displayedText : bubbleText} />
                    {isStreaming && !isUser && (
                        <span className="inline-block w-[6px] h-[1em] ml-0.5 align-[-2px] bg-dark-300 animate-pulse" />
                    )}
                </div>

                {/* Agent steps toggle */}
                {hasSteps && (
                    <div className="mt-2">
                        <button
                            onClick={() => setShowSteps(!showSteps)}
                            className="flex items-center gap-1.5 text-xs text-dark-400 hover:text-dark-200 transition-colors"
                        >
                            {showSteps ? (
                                <ChevronDown className="w-3 h-3" />
                            ) : (
                                <ChevronRight className="w-3 h-3" />
                            )}
                            <Brain className="w-3 h-3" />
                            <span>{message.tool_calls!.length} agent step{message.tool_calls!.length > 1 ? 's' : ''}</span>
                        </button>

                        {showSteps && (
                            <div className="mt-2 space-y-2">
                                {message.tool_calls!.map((step, i) => (
                                    <AgentStepCard key={i} step={step} />
                                ))}
                            </div>
                        )}
                    </div>
                )}

                {/* Activity references */}
                {hasActivities && (
                    <div className="mt-3 space-y-1.5">
                        <p className="text-xs text-dark-500 font-medium uppercase tracking-wide">Referenced Activities</p>
                        <div className="flex flex-wrap gap-2">
                            {message.activities!.slice(0, 8).map((act, i) => (
                                <ActivityCard key={i} activity={act} />
                            ))}
                            {message.activities!.length > 8 && (
                                <span className="text-xs text-dark-500 self-center">
                                    +{message.activities!.length - 8} more
                                </span>
                            )}
                        </div>
                    </div>
                )}

                {/* Timestamp */}
                <p className={`text-[10px] mt-1 ${isUser ? 'text-right text-primary-300' : 'text-dark-500'}`}>
                    {formatTime(message.created_at)}
                </p>
            </div>
        </div>
    );
}

function MarkdownMessage({ text }: { text: string }) {
    return (
        <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={{
                h1: ({ children }) => <h1 className="text-base font-semibold mt-2 mb-1">{children}</h1>,
                h2: ({ children }) => <h2 className="text-base font-semibold mt-2 mb-1">{children}</h2>,
                h3: ({ children }) => <h3 className="text-sm font-semibold mt-2 mb-1">{children}</h3>,
                h4: ({ children }) => <h4 className="text-sm font-medium mt-1 mb-1">{children}</h4>,
                h5: ({ children }) => <h5 className="text-sm font-medium mt-1 mb-1">{children}</h5>,
                h6: ({ children }) => <h6 className="text-sm font-medium mt-1 mb-1">{children}</h6>,
                p: ({ children }) => <p className="text-sm leading-relaxed whitespace-pre-wrap mb-2 last:mb-0">{children}</p>,
                ul: ({ children }) => <ul className="list-disc pl-5 text-sm my-1 space-y-1">{children}</ul>,
                ol: ({ children }) => <ol className="list-decimal pl-5 text-sm my-1 space-y-1">{children}</ol>,
                li: ({ children }) => <li>{children}</li>,
                a: ({ href, children }) => (
                    <a href={href} target="_blank" rel="noreferrer" className="text-primary-300 underline hover:text-primary-200">
                        {children}
                    </a>
                ),
                strong: ({ children }) => <strong className="font-semibold text-white">{children}</strong>,
                em: ({ children }) => <em className="italic">{children}</em>,
                code: ({ className, children }) =>
                    !className ? (
                        <code className="px-1 py-0.5 rounded bg-dark-900 text-xs">{children}</code>
                    ) : (
                        <code className={className}>{children}</code>
                    ),
                pre: ({ children }) => (
                    <pre className="text-xs bg-dark-950 border border-dark-700 rounded p-3 overflow-x-auto whitespace-pre-wrap my-2">
                        {children}
                    </pre>
                ),
                table: ({ children }) => (
                    <div className="overflow-x-auto my-2">
                        <table className="min-w-full text-xs border border-dark-700 rounded overflow-hidden">{children}</table>
                    </div>
                ),
                thead: ({ children }) => <thead className="bg-dark-900">{children}</thead>,
                th: ({ children }) => <th className="px-2 py-1 text-left border-b border-dark-700">{children}</th>,
                td: ({ children }) => <td className="px-2 py-1 align-top border-b border-dark-800">{children}</td>,
                blockquote: ({ children }) => (
                    <blockquote className="border-l-2 border-dark-600 pl-3 italic text-dark-300 my-2">{children}</blockquote>
                ),
            }}
        >
            {text}
        </ReactMarkdown>
    );
}

function splitThinkingContent(content: string): { answerText: string; thinkingText: string } {
    const openTag = /<think>/i;
    const closeTag = /<\/think>/i;
    const openMatch = openTag.exec(content);
    if (!openMatch) {
        return { answerText: stripToolJsonPayloads(content), thinkingText: '' };
    }

    const openIndex = openMatch.index;
    const openEnd = openIndex + openMatch[0].length;
    const closeMatch = closeTag.exec(content.slice(openEnd));

    if (!closeMatch) {
        const before = content.slice(0, openIndex).trim();
        const thinking = sanitizeThinkingText(content.slice(openEnd).trim());
        return { answerText: before, thinkingText: thinking };
    }

    const closeStart = openEnd + closeMatch.index;
    const closeEnd = closeStart + closeMatch[0].length;
    const thinking = sanitizeThinkingText(content.slice(openEnd, closeStart).trim());
    const answer = `${content.slice(0, openIndex)}${content.slice(closeEnd)}`.trim();
    return { answerText: stripToolJsonPayloads(answer), thinkingText: thinking };
}

function sanitizeThinkingText(raw: string): string {
    const text = raw.trim();
    if (!text) return '';
    const candidate = extractJsonObject(text) || text;
    try {
        const parsed = JSON.parse(candidate) as { tool?: string; args?: unknown; reasoning?: string };
        if (parsed && typeof parsed === 'object' && parsed.tool && parsed.args) {
            // Hide raw tool-call JSON from user-facing thinking panel.
            if (typeof parsed.reasoning === 'string' && parsed.reasoning.trim()) {
                return parsed.reasoning.trim();
            }
            return '';
        }
    } catch {
        // Not JSON, keep as-is.
    }
    const compact = text
        .replace(/\bThinking\.\.\.\s*$/i, '')
        .replace(/\s{2,}/g, ' ')
        .trim();

    if (!compact) return '';

    // Hide internal planning/meta-reasoning leaked by some reasoning models.
    const lower = compact.toLowerCase();
    const looksInternal =
        lower.includes('the user ') ||
        lower.includes('user says') ||
        lower.includes('i should') ||
        lower.includes('let me ') ||
        lower.includes('likely they') ||
        lower.includes('we need to') ||
        lower.includes('then we') ||
        lower.includes('let\'s call') ||
        lower.includes('tool output') ||
        lower.includes('call get_') ||
        lower.includes('reasoning models');

    if (looksInternal) {
        return 'Analyzing your request and checking the relevant activity data.';
    }

    // Keep thinking concise for UI readability.
    const firstSentence = compact.split(/(?<=[.!?])\s+/)[0]?.trim() || compact;
    return firstSentence.slice(0, 180);
}

function extractJsonObject(text: string): string | null {
    const start = text.indexOf('{');
    const end = text.lastIndexOf('}');
    if (start >= 0 && end > start) {
        return text.slice(start, end + 1);
    }
    return null;
}

function stripToolJsonPayloads(text: string): string {
    const pattern = /\{[\s\S]*?"tool"\s*:[\s\S]*?"args"\s*:[\s\S]*?\}/g;
    return text.replace(pattern, '').replace(/\n{3,}/g, '\n\n');
}

function AgentStepCard({ step }: { step: AgentStep }) {
    const [expanded, setExpanded] = useState(false);

    const toolIcon = () => {
        switch (step.tool_name) {
            case 'query_activities': return <Clock className="w-3.5 h-3.5 text-blue-400" />;
            case 'search_ocr': return <Monitor className="w-3.5 h-3.5 text-green-400" />;
            case 'get_usage_stats': return <Wrench className="w-3.5 h-3.5 text-yellow-400" />;
            default: return <Wrench className="w-3.5 h-3.5 text-dark-400" />;
        }
    };

    const toolLabel = () => {
        switch (step.tool_name) {
            case 'query_activities': return 'Queried Activities';
            case 'search_ocr': return 'Searched Screen Text';
            case 'get_usage_stats': return 'Fetched Usage Stats';
            default: return step.tool_name;
        }
    };

    // Parse result to show a summary
    const resultSummary = () => {
        try {
            const data = JSON.parse(step.tool_result);
            if (Array.isArray(data)) {
                return `${data.length} result${data.length !== 1 ? 's' : ''}`;
            }
            return 'Data received';
        } catch {
            return step.tool_result.length > 100
                ? `${step.tool_result.substring(0, 100)}...`
                : step.tool_result;
        }
    };

    return (
        <div className="bg-dark-900 border border-dark-700 rounded-lg overflow-hidden">
            <button
                onClick={() => setExpanded(!expanded)}
                className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-dark-800/50 transition-colors"
            >
                {/* Pipeline indicator */}
                <div className="flex items-center gap-1.5">
                    <div className="w-5 h-5 rounded-full bg-dark-700 flex items-center justify-center text-[10px] font-bold text-dark-300">
                        {step.turn}
                    </div>
                    {toolIcon()}
                </div>

                <div className="flex-1 min-w-0">
                    <span className="text-xs font-medium text-dark-200">{toolLabel()}</span>
                    <span className="text-[10px] text-dark-500 ml-2">→ {resultSummary()}</span>
                </div>

                {expanded ? (
                    <ChevronDown className="w-3 h-3 text-dark-500 flex-shrink-0" />
                ) : (
                    <ChevronRight className="w-3 h-3 text-dark-500 flex-shrink-0" />
                )}
            </button>

            {expanded && (
                <div className="px-3 pb-3 space-y-2 border-t border-dark-700">
                    {step.reasoning && (
                        <div className="mt-2">
                            <p className="text-[10px] text-dark-500 uppercase tracking-wide mb-0.5">Reasoning</p>
                            <p className="text-xs text-dark-300 italic">{step.reasoning}</p>
                        </div>
                    )}
                    <div>
                        <p className="text-[10px] text-dark-500 uppercase tracking-wide mb-0.5">Arguments</p>
                        <pre className="text-[11px] text-dark-300 bg-dark-950 rounded p-2 overflow-x-auto">
                            {JSON.stringify(step.tool_args, null, 2)}
                        </pre>
                    </div>
                    <div>
                        <p className="text-[10px] text-dark-500 uppercase tracking-wide mb-0.5">Result</p>
                        <pre className="text-[11px] text-dark-300 bg-dark-950 rounded p-2 overflow-x-auto max-h-40 overflow-y-auto">
                            {(() => {
                                try {
                                    return JSON.stringify(JSON.parse(step.tool_result), null, 2);
                                } catch {
                                    return step.tool_result;
                                }
                            })()}
                        </pre>
                    </div>
                </div>
            )}
        </div>
    );
}

function ActivityCard({ activity }: { activity: ActivityRef }) {
    const hasMedia = activity.media && activity.media.title;

    return (
        <div className="flex items-center gap-2 bg-dark-800/60 border border-dark-700 rounded-lg px-3 py-1.5 hover:bg-dark-700/60 transition-colors cursor-pointer">
            {hasMedia ? (
                <Music className="w-3.5 h-3.5 text-green-400 flex-shrink-0" />
            ) : (
                <Monitor className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />
            )}
            <div className="min-w-0">
                <p className="text-xs text-dark-200 font-medium truncate max-w-[200px]">
                    {hasMedia
                        ? `${activity.media!.title} – ${activity.media!.artist}`
                        : activity.title || activity.app}
                </p>
                <p className="text-[10px] text-dark-500">
                    {activity.app} · {formatTime(activity.time)}
                </p>
            </div>
        </div>
    );
}
