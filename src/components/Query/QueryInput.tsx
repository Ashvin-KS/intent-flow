import { useState } from 'react';
import { Search, Send } from 'lucide-react';
import { Button } from '../common';
import { executeQuery } from '../../services/tauri';
import type { QueryResult } from '../../types';

interface QueryInputProps {
  onResult?: (result: QueryResult) => void;
  placeholder?: string;
}

export function QueryInput({ onResult, placeholder = "Ask anything about your activities..." }: QueryInputProps) {
  const [query, setQuery] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim() || isLoading) return;

    setIsLoading(true);
    setError(null);

    try {
      const result = await executeQuery(query.trim());
      onResult?.(result);
      setQuery('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to execute query');
    } finally {
      setIsLoading(false);
    }
  };

  const suggestions = [
    "What did I do yesterday?",
    "When did I last use VS Code?",
    "How much time did I spend coding today?",
    "What websites did I visit this morning?",
  ];

  return (
    <div className="w-full">
      <form onSubmit={handleSubmit} className="relative">
        <div className="relative">
          <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-dark-400" />
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
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

      {error && (
        <p className="mt-2 text-sm text-red-400">{error}</p>
      )}

      {/* Quick suggestions */}
      <div className="mt-3 flex flex-wrap gap-2">
        {suggestions.map((suggestion) => (
          <button
            key={suggestion}
            onClick={() => setQuery(suggestion)}
            className="px-3 py-1.5 text-sm bg-dark-800 text-dark-300 rounded-lg hover:bg-dark-700 hover:text-white transition-colors"
          >
            {suggestion}
          </button>
        ))}
      </div>
    </div>
  );
}
