import { useColorScheme } from "react-native";
import { Colors, type ThemeColors } from "../constants/theme";

export function useTheme() {
  const colorScheme = useColorScheme();
  const isDark = colorScheme === "dark";
  const theme: ThemeColors = isDark ? Colors.dark : Colors.light;

  return {
    theme,
    isDark,
  };
}
