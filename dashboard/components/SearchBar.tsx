"use client";

import { useCallback, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { api, type UserSearchResult } from "@/lib/api";

export default function SearchBar() {
  const router = useRouter();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<UserSearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const debouncedSearch = useCallback((value: string) => {
    if (timerRef.current) clearTimeout(timerRef.current);
    if (value.trim().length < 2) {
      setResults([]);
      setOpen(false);
      return;
    }
    timerRef.current = setTimeout(async () => {
      setLoading(true);
      try {
        const data = await api.searchUsers(value.trim());
        setResults(data);
        setOpen(data.length > 0);
      } catch {
        setResults([]);
      } finally {
        setLoading(false);
      }
    }, 300);
  }, []);

  const handleSelect = (username: string) => {
    setQuery("");
    setOpen(false);
    router.push(`/dashboard/transactions?user=${encodeURIComponent(username)}`);
  };

  return (
    <div className="relative">
      <input
        type="text"
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          debouncedSearch(e.target.value);
        }}
        onFocus={() => results.length > 0 && setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 200)}
        placeholder="Search users by username…"
        className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm
                   focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-200"
        aria-label="Search users by username"
      />
      {loading && (
        <span className="absolute right-3 top-2.5 h-4 w-4 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600" />
      )}
      {open && (
        <ul className="absolute z-30 mt-1 max-h-60 w-full overflow-auto rounded-lg border border-slate-200 bg-white shadow-lg">
          {results.map((u) => (
            <li key={u.username}>
              <button
                type="button"
                onMouseDown={() => handleSelect(u.username)}
                className="flex w-full flex-col px-4 py-2.5 text-left text-sm hover:bg-indigo-50"
              >
                <span className="font-medium text-slate-900">{u.username}</span>
                <span className="text-xs text-slate-500 truncate">{u.public_key}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
