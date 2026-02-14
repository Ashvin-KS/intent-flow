import { useState } from 'react';
import { Send, Sparkles } from 'lucide-react';
import { Button } from '../common';
import { parseIntent, executeIntent } from '../../services/tauri';
import type { Intent } from '../../types';

interface IntentInputProps {
    placeholder?: string;
}

export function IntentInput({ placeholder = "Tell me what you want to do... (e.g. 'time to code', 'I'm bored')" }: IntentInputProps) {
    const [input, setInput] = useState('');
    const [intent, setIntent] = useState<Intent | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [isExecuting, setIsExecuting] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [success, setSuccess] = useState(false);

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!input.trim() || isLoading) return;

        setIsLoading(true);
        setError(null);
        setIntent(null);
        setSuccess(false);

        try {
            const result = await parseIntent(input.trim());
            setIntent(result);
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to parse intent');
        } finally {
            setIsLoading(false);
        }
    };

    const handleExecute = async () => {
        if (!intent) return;
        setIsExecuting(true);
        try {
            await executeIntent(intent);
            setSuccess(true);
            setInput('');
            setTimeout(() => {
                setIntent(null);
                setSuccess(false);
            }, 2000);
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to execute actions');
        } finally {
            setIsExecuting(false);
        }
    };

    const quickIntents = [
        { label: '💻 Code mode', input: "let's code" },
        { label: '🎮 Entertainment', input: "I'm bored" },
        { label: '🎯 Focus', input: 'focus time' },
        { label: '📚 Learn', input: 'time to learn' },
    ];

    return (
        <div className="w-full">
            <form onSubmit={handleSubmit} className="relative">
                <div className="relative">
                    <Sparkles className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-primary-400" />
                    <input
                        type="text"
                        value={input}
                        onChange={(e) => setInput(e.target.value)}
                        placeholder={placeholder}
                        className="w-full pl-12 pr-24 py-3 bg-dark-800 border border-dark-700 rounded-xl text-white placeholder-dark-400 focus:outline-none focus:ring-2 focus:ring-primary-500 focus:border-transparent"
                        disabled={isLoading}
                    />
                    <Button
                        type="submit"
                        variant="primary"
                        size="sm"
                        className="absolute right-2 top-1/2 -translate-y-1/2"
                        isLoading={isLoading}
                    >
                        {!isLoading && <Send className="w-4 h-4" />}
                    </Button>
                </div>
            </form>

            {/* Quick intents */}
            <div className="mt-3 flex flex-wrap gap-2">
                {quickIntents.map((qi) => (
                    <button
                        key={qi.label}
                        onClick={() => setInput(qi.input)}
                        className="px-3 py-1.5 text-sm bg-dark-800 text-dark-300 rounded-lg hover:bg-dark-700 hover:text-white transition-colors"
                    >
                        {qi.label}
                    </button>
                ))}
            </div>

            {error && <p className="mt-2 text-sm text-red-400">{error}</p>}

            {/* Intent result */}
            {intent && (
                <div className="mt-4 p-4 bg-dark-800 rounded-xl border border-dark-700 animate-fade-in">
                    <div className="flex items-center justify-between mb-3">
                        <div>
                            <span className="text-xs text-dark-400">Detected Intent:</span>
                            <p className="text-white font-semibold capitalize">
                                {intent.intent_type.replace('_', ' ')}
                            </p>
                        </div>
                        <span className="px-2 py-1 text-xs rounded-full bg-primary-600/20 text-primary-400">
                            {Math.round(intent.confidence * 100)}% confidence
                        </span>
                    </div>

                    {intent.suggested_actions.length > 0 && (
                        <div className="space-y-2 mb-3">
                            <p className="text-xs text-dark-400">Suggested Actions:</p>
                            {intent.suggested_actions.map((action, i) => (
                                <div
                                    key={i}
                                    className="flex items-center gap-2 px-3 py-2 bg-dark-700/50 rounded-lg"
                                >
                                    <span className="text-xs text-primary-400 font-mono uppercase">
                                        {action.action_type.replace('_', ' ')}
                                    </span>
                                    <span className="text-sm text-dark-300">{action.target}</span>
                                </div>
                            ))}
                        </div>
                    )}

                    <Button
                        variant="primary"
                        size="sm"
                        onClick={handleExecute}
                        isLoading={isExecuting}
                        className="w-full"
                    >
                        {success ? '✓ Done!' : 'Execute Actions'}
                    </Button>
                </div>
            )}
        </div>
    );
}
