import { useState } from 'react';
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
}

export function ChatMessage({ message }: ChatMessageProps) {
    const [showSteps, setShowSteps] = useState(false);
    const isUser = message.role === 'user';
    const hasSteps = message.tool_calls && message.tool_calls.length > 0;
    const hasActivities = message.activities && message.activities.length > 0;

    return (
        <div className={`flex ${isUser ? 'justify-end' : 'justify-start'} mb-4`}>
            <div className={`max-w-[85%] ${isUser ? 'order-1' : 'order-1'}`}>
                {/* Message bubble */}
                <div
                    className={`rounded-2xl px-4 py-3 ${isUser
                        ? 'bg-primary-600 text-white rounded-br-md'
                        : 'bg-dark-800 text-dark-100 rounded-bl-md border border-dark-700'
                        }`}
                >
                    <p className="text-sm whitespace-pre-wrap leading-relaxed">{message.content}</p>
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
