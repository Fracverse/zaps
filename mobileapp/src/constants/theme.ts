export const Spacing = {
  xs: 4,
  sm: 8,
  md: 12,
  lg: 16,
  xl: 20,
  "2xl": 24,
  "3xl": 32,
  "4xl": 40,
  "5xl": 48,
  inputHeight: 56,
  buttonHeight: 56,
};

export const BorderRadius = {
  sm: 8,
  md: 12,
  lg: 16,
  xl: 20,
  full: 9999,
};

export const Colors = {
  light: {
    primary: "#1A4B4A",
    secondary: "#80FA98",
    text: "#000000",
    textSecondary: "#666666",
    background: "#FFFFFF",
    backgroundRoot: "#FFFFFF",
    backgroundDefault: "#F5F5F5",
    border: "#E0E0E0",
    success: "#22C55E",
    warning: "#F59E0B",
    error: "#EF4444",
  },
  dark: {
    primary: "#80FA98",
    secondary: "#1A4B4A",
    text: "#FFFFFF",
    textSecondary: "#A0A0A0",
    background: "#1A1A1A",
    backgroundRoot: "#000000",
    backgroundDefault: "#2A2A2A",
    border: "#333333",
    success: "#22C55E",
    warning: "#F59E0B",
    error: "#EF4444",
  },
};

export type ThemeColors = typeof Colors.light;
