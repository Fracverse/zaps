import React from "react";
import {
  TouchableOpacity,
  Text,
  StyleSheet,
  ActivityIndicator,
  ViewStyle,
  TextStyle,
} from "react-native";
import { COLORS } from "../constants/colors";

interface ButtonProps {
  title: string;
  onPress: () => void;
  variant?: "primary" | "secondary" | "outline" | "ghost";
  loading?: boolean;
  disabled?: boolean;
  style?: ViewStyle;
  textStyle?: TextStyle;
  icon?: React.ReactNode;
}

export const Button = ({
  title,
  onPress,
  variant = "primary",
  loading = false,
  disabled = false,
  style,
  textStyle,
  icon,
}: ButtonProps) => {
  const getBackgroundColor = () => {
    switch (variant) {
      case "primary":
        return COLORS.primary;
      case "secondary":
        return COLORS.secondary;
      case "outline":
      case "ghost":
        return "transparent";
      default:
        return COLORS.primary;
    }
  };

  const getTextColor = () => {
    switch (variant) {
      case "primary":
        return COLORS.secondary;
      case "secondary":
        return COLORS.primary;
      case "outline":
      case "ghost":
        return COLORS.primary;
      default:
        return COLORS.white;
    }
  };

  const getBorder = () => {
    if (variant === "outline") {
      return { borderWidth: 1, borderColor: COLORS.primary };
    }
    return {};
  };

  return (
    <TouchableOpacity
      style={[
        styles.button,
        { backgroundColor: getBackgroundColor() },
        getBorder(),
        style,
        (loading || disabled) && styles.disabled,
      ]}
      onPress={onPress}
      disabled={loading || disabled}
      activeOpacity={0.8}
    >
      {loading ? (
        <ActivityIndicator color={getTextColor()} />
      ) : (
        <>
          {icon}
          <Text style={[styles.text, { color: getTextColor() }, textStyle]}>
            {title}
          </Text>
        </>
      )}
    </TouchableOpacity>
  );
};

const styles = StyleSheet.create({
  button: {
    height: 56,
    borderRadius: 28,
    justifyContent: "center",
    alignItems: "center",
    flexDirection: "row",
    paddingHorizontal: 24,
    width: "100%",
  },
  text: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    textAlign: "center",
  },
  disabled: {
    opacity: 0.7,
  },
});
