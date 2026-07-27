import React from "react";
import { View, Text, Image, StyleSheet } from "react-native";
import { COLORS } from "../constants/colors";

interface AvatarProps {
  uri?: string | null;
  name: string;
  size?: number;
}

export const Avatar = React.memo(function Avatar({
  uri,
  name,
  size = 64,
}: AvatarProps) {
  const initials = name
    .split(/[\s._-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w.charAt(0).toUpperCase())
    .join("");

  const fontSize = Math.round(size * 0.38);

  if (uri) {
    return (
      <Image
        source={{ uri }}
        style={[styles.image, { width: size, height: size, borderRadius: size / 2 }]}
        accessibilityLabel={`Avatar for ${name}`}
      />
    );
  }

  return (
    <View
      style={[
        styles.fallback,
        { width: size, height: size, borderRadius: size / 2 },
      ]}
      accessibilityLabel={`Avatar placeholder for ${name}`}
    >
      <Text style={[styles.initials, { fontSize }]}>
        {initials || "?"}
      </Text>
    </View>
  );
});

const styles = StyleSheet.create({
  image: {
    backgroundColor: COLORS.gray,
  },
  fallback: {
    backgroundColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
  },
  initials: {
    fontFamily: "Outfit_700Bold",
    color: COLORS.secondary,
  },
});
