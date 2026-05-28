import React, { useRef } from "react";
import {
  TextInput,
  View,
  Text,
  StyleSheet,
  TextInputProps,
  TouchableOpacity,
} from "react-native";
import { COLORS } from "../constants/colors";

interface InputProps extends TextInputProps {
  label?: string;
  error?: string;
  /** Accessibility hint for screen readers. */
  accessibilityHint?: string;
}

export const Input = React.memo(function Input({
  label,
  error,
  style,
  accessibilityHint,
  ...props
}: InputProps) {
  const inputRef = useRef<TextInput>(null);
  const errorId = error ? `${props.testID ?? "input"}-error` : undefined;

  return (
    <View style={styles.container}>
      {label && (
        <TouchableOpacity
          accessible={false}
          onPress={() => inputRef.current?.focus()}
        >
          <Text style={styles.label}>{label}</Text>
        </TouchableOpacity>
      )}
      <TextInput
        ref={inputRef}
        style={[styles.input, error ? styles.inputError : null, style]}
        placeholderTextColor="#999"
        accessible
        accessibilityLabel={label}
        accessibilityHint={accessibilityHint}
        accessibilityInvalid={!!error}
        // Link error message to input for screen readers
        {...(errorId ? { accessibilityDescribedBy: errorId } : {})}
        {...props}
      />
      {error && (
        <Text
          nativeID={errorId}
          style={styles.errorText}
          accessibilityRole="alert"
          accessibilityLiveRegion="polite"
        >
          {error}
        </Text>
      )}
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    marginBottom: 16,
    width: "100%",
  },
  label: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: "#000",
    marginBottom: 8,
  },
  input: {
    // Minimum 44pt height for touch target (WCAG 2.5.5)
    minHeight: 44,
    height: 56,
    backgroundColor: "#FFFFFF",
    borderRadius: 28,
    paddingHorizontal: 24,
    fontSize: 16,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
    borderWidth: 1,
    borderColor: "#eee",
  },
  inputError: {
    // High-contrast error border (WCAG 1.4.3)
    borderColor: "#CC0000",
    borderWidth: 2,
  },
  errorText: {
    color: "#CC0000",
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    marginTop: 4,
    marginLeft: 12,
  },
});
