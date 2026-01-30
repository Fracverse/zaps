import React from "react";
import { Text, type TextProps, StyleSheet } from "react-native";
import { useTheme } from "../hooks/useTheme";

export type ThemedTextProps = TextProps & {
  type?: "default" | "title" | "subtitle" | "link";
};

export function ThemedText({
  style,
  type = "default",
  ...rest
}: ThemedTextProps) {
  const { theme } = useTheme();

  return (
    <Text
      style={[
        { color: theme.text },
        type === "default" && styles.default,
        type === "title" && styles.title,
        type === "subtitle" && styles.subtitle,
        type === "link" && styles.link,
        style,
      ]}
      {...rest}
    />
  );
}

const styles = StyleSheet.create({
  default: {
    fontSize: 16,
  },
  title: {
    fontSize: 24,
    fontWeight: "700",
  },
  subtitle: {
    fontSize: 18,
    fontWeight: "600",
  },
  link: {
    fontSize: 16,
    fontWeight: "500",
  },
});
