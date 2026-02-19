import { useState, useEffect, useCallback } from 'react';

export interface FavoriteModel {
    id: string;
    name: string;
}

const STORAGE_KEY = 'intentflow_favorite_models';

function loadFromStorage(): FavoriteModel[] {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (raw) return JSON.parse(raw);
    } catch { }
    return [];
}

function saveToStorage(models: FavoriteModel[]) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(models));
}

export function useFavoriteModels() {
    const [favorites, setFavorites] = useState<FavoriteModel[]>(loadFromStorage);

    useEffect(() => {
        saveToStorage(favorites);
    }, [favorites]);

    const addFavorite = useCallback((model: FavoriteModel) => {
        setFavorites((prev) => {
            if (prev.some((m) => m.id === model.id)) return prev;
            if (prev.length >= 5) return prev; // max 5
            return [...prev, model];
        });
    }, []);

    const removeFavorite = useCallback((modelId: string) => {
        setFavorites((prev) => prev.filter((m) => m.id !== modelId));
    }, []);

    const isFavorite = useCallback(
        (modelId: string) => favorites.some((m) => m.id === modelId),
        [favorites]
    );

    return { favorites, addFavorite, removeFavorite, isFavorite, setFavorites };
}
