import { useContext } from "react";
import { useColorScheme } from "react-native";
import { Colors, type ThemeColors } from "../constants/theme";
import { ThemeContext } from "../contexts/ThemeProvider";

/**
 * Resolved theme for the current screen.
 *
 * Prefers the active `ThemeProvider` context (which honours the user's
 * persisted light / dark / high-contrast preference and updates immediately
 * on change). Falls back to reading the system colour scheme when no provider
 * is mounted, so standalone components and tests keep working.
 *
 * Both hooks are called unconditionally to keep hook ordering stable; the
 * context simply takes precedence when it is available.
 */
export function useTheme() {
  const ctx = useContext(ThemeContext);
  const systemIsDark = useColorScheme() === "dark";

  const isDark = ctx ? ctx.isDark : systemIsDark;
  const isHighContrast = ctx?.isHighContrast ?? false;
  const theme: ThemeColors = ctx ? ctx.theme : isDark ? Colors.dark : Colors.light;

  return {
    theme,
    isDark,
    isHighContrast,
  };
}