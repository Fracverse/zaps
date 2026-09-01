import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { useColorScheme } from "react-native";
import AsyncStorage from "@react-native-async-storage/async-storage";
import { Colors, type ThemeColors } from "../constants/theme";

/**
 * Theme preference the user can pick from (and persist) in Settings.
 *
 * - `system`  → follow the device light/dark setting
 * - `light`   → always light
 * - `dark`    → always dark
 * - `high-contrast` → dark palette with WCAG-friendly contrast boost
 */
export type ThemePreference = "system" | "light" | "dark" | "high-contrast";

const STORAGE_KEY = "zaps:theme_preference";

interface ThemeContextValue {
  /** The resolved palette for the current preference. */
  theme: ThemeColors;
  /** Whether the resolved mode is dark. */
  isDark: boolean;
  /** Whether the high-contrast palette is active. */
  isHighContrast: boolean;
  /** The user's current (possibly "system") preference. */
  preference: ThemePreference;
  /** Persist a new preference and re-resolve the theme immediately. */
  setPreference: (preference: ThemePreference) => void;
}

export const ThemeContext = createContext<ThemeContextValue | null>(null);

function resolveTheme(
  preference: ThemePreference,
  systemIsDark: boolean
): { theme: ThemeColors; isDark: boolean; isHighContrast: boolean } {
  switch (preference) {
    case "light":
      return { theme: Colors.light, isDark: false, isHighContrast: false };
    case "dark":
      return { theme: Colors.dark, isDark: true, isHighContrast: false };
    case "high-contrast":
      return {
        theme: Colors.highContrast,
        isDark: true,
        isHighContrast: true,
      };
    case "system":
    default:
      return systemIsDark
        ? { theme: Colors.dark, isDark: true, isHighContrast: false }
        : { theme: Colors.light, isDark: false, isHighContrast: false };
  }
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const systemIsDark = useColorScheme() === "dark";
  const [preference, setPreferenceState] = useState<ThemePreference>("system");

  // Rehydrate the stored preference on first mount.
  useEffect(() => {
    let cancelled = false;
    AsyncStorage.getItem(STORAGE_KEY)
      .then((raw) => {
        if (cancelled) return;
        if (
          raw === "light" ||
          raw === "dark" ||
          raw === "high-contrast" ||
          raw === "system"
        ) {
          setPreferenceState(raw);
        }
      })
      .catch(() => {
        // Non-fatal — default to "system".
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const setPreference = useCallback((next: ThemePreference) => {
    setPreferenceState(next);
    AsyncStorage.setItem(STORAGE_KEY, next).catch(() => {
      // Persistence failures must not block the in-session toggle.
    });
  }, []);

  const value = useMemo<ThemeContextValue>(() => {
    const resolved = resolveTheme(preference, systemIsDark);
    return {
      ...resolved,
      preference,
      setPreference,
    };
  }, [preference, systemIsDark, setPreference]);

  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}

export function useThemeContext(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useThemeContext must be used within a ThemeProvider");
  }
  return ctx;
}
