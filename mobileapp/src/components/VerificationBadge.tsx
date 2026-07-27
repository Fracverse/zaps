import React from "react";
import { View, StyleSheet } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../constants/colors";

interface VerificationBadgeProps {
  size?: number;
}

export const VerificationBadge = React.memo(function VerificationBadge({
  size = 18,
}: VerificationBadgeProps) {
  return (
    <View
      style={[styles.badge, { width: size, height: size, borderRadius: size / 2 }]}
      accessibilityLabel="Verified recipient"
      accessibilityRole="text"
    >
      <Ionicons name="shield-checkmark" size={Math.round(size * 0.7)} color={COLORS.white} />
    </View>
  );
});

const styles = StyleSheet.create({
  badge: {
    backgroundColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
    marginLeft: 4,
  },
});
