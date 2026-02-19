import { useState, useEffect, useCallback } from 'react';
import { getActivities, getActivityStats, getCurrentActivity } from '../services/tauri';
import type { Activity, ActivityStats } from '../types';
import { getDayRange } from '../lib/utils';

export function useActivities(daysAgo: number = 0) {
  const [activities, setActivities] = useState<Activity[]>([]);
  const [stats, setStats] = useState<ActivityStats | null>(null);
  const [currentActivity, setCurrentActivity] = useState<Activity | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const { start, end } = getDayRange(daysAgo);
      const [activitiesData, statsData, current] = await Promise.all([
        getActivities(start, end),
        getActivityStats(start, end),
        getCurrentActivity(),
      ]);
      setActivities(activitiesData);
      setStats(statsData);
      setCurrentActivity(current);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load activities');
    } finally {
      setIsLoading(false);
    }
  }, [daysAgo]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { activities, stats, currentActivity, isLoading, error, refresh };
}
