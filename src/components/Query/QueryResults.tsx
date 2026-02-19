import { Clock, ArrowRight } from 'lucide-react';
import type { QueryResult, QueryItem } from '../../types';
import { formatDate } from '../../lib/utils';

interface QueryResultsProps {
  result: QueryResult | null;
}

export function QueryResults({ result }: QueryResultsProps) {
  if (!result) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-dark-400">
        <Clock className="w-12 h-12 mb-4 opacity-50" />
        <p className="text-lg">Ask a question to see results</p>
        <p className="text-sm mt-1">Your activity history is ready to be explored</p>
      </div>
    );
  }

  return (
    <div className="w-full animate-fade-in">
      {/* Summary */}
      <div className="mb-4 p-4 bg-dark-800 rounded-lg border border-dark-700">
        <p className="text-white">{result.summary}</p>
        <p className="text-xs text-dark-400 mt-2">
          Query: "{result.query}" • {formatDate(result.timestamp)}
        </p>
      </div>

      {/* Results list */}
      {result.results.length > 0 ? (
        <div className="space-y-2">
          {result.results.map((item, index) => (
            <QueryResultItem key={index} item={item} />
          ))}
        </div>
      ) : (
        <div className="text-center py-8 text-dark-400">
          <p>No activities found for this query</p>
        </div>
      )}
    </div>
  );
}

function QueryResultItem({ item }: { item: QueryItem }) {
  return (
    <div className="flex items-start gap-3 p-3 bg-dark-800/50 rounded-lg hover:bg-dark-800 transition-colors">
      <div className="flex-shrink-0 w-16 text-right">
        <span className="text-sm text-primary-400 font-mono">{item.time_str}</span>
      </div>
      <ArrowRight className="flex-shrink-0 w-4 h-4 text-dark-500 mt-0.5" />
      <div className="flex-1 min-w-0">
        <p className="text-white truncate">{item.activity}</p>
        <div className="flex items-center gap-2 mt-1">
          <span className="text-xs text-dark-400">{item.duration}</span>
          {item.details && (
            <>
              <span className="text-dark-600">•</span>
              <span className="text-xs text-dark-400 truncate">{item.details}</span>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
