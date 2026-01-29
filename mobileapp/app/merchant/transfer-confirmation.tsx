import React, { useState, useEffect } from "react";
import {
  View,
  StyleSheet,
  Pressable,
  TextInput,
  Animated,
  Keyboard,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { Feather } from "@expo/vector-icons";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";

import { ThemedText } from "@/src/components/ThemedText";
import { useTheme } from "@/src/hooks/useTheme";
import { Spacing, BorderRadius } from "@/src/constants/theme";

const PIN_LENGTH = 4;

export default function TransferConfirmationScreen() {
  const insets = useSafeAreaInsets();
  const { theme } = useTheme();
  const router = useRouter();
  const [pin, setPin] = useState("");
  const [error, setError] = useState(false);
  const shakeAnim = React.useRef(new Animated.Value(0)).current;
  const scaleAnims = React.useRef(
    Array(PIN_LENGTH)
      .fill(0)
      .map(() => new Animated.Value(1))
  ).current;

  useEffect(() => {
    if (pin.length === PIN_LENGTH) {
      // Simulate PIN verification
      setTimeout(() => {
        if (pin === "1234") {
          // Correct PIN
          Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
          router.push("/merchant/success");
        } else {
          // Incorrect PIN
          Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error);
          setError(true);
          shakeAnimation();
          setPin("");
          setTimeout(() => setError(false), 2000);
        }
      }, 300);
    }
  }, [pin]);

  const shakeAnimation = () => {
    Animated.sequence([
      Animated.timing(shakeAnim, {
        toValue: 10,
        duration: 50,
        useNativeDriver: true,
      }),
      Animated.timing(shakeAnim, {
        toValue: -10,
        duration: 50,
        useNativeDriver: true,
      }),
      Animated.timing(shakeAnim, {
        toValue: 10,
        duration: 50,
        useNativeDriver: true,
      }),
      Animated.timing(shakeAnim, {
        toValue: 0,
        duration: 50,
        useNativeDriver: true,
      }),
    ]).start();
  };

  const handlePinInput = (value: string) => {
    if (error) return;
    if (value.length <= PIN_LENGTH) {
      setPin(value);
      if (value.length > pin.length && value.length <= PIN_LENGTH) {
        Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
        // Animate the dot
        const index = value.length - 1;
        Animated.sequence([
          Animated.timing(scaleAnims[index], {
            toValue: 1.3,
            duration: 150,
            useNativeDriver: true,
          }),
          Animated.timing(scaleAnims[index], {
            toValue: 1,
            duration: 150,
            useNativeDriver: true,
          }),
        ]).start();
      }
    }
  };

  return (
    <View style={[styles.container, { backgroundColor: theme.backgroundRoot }]}>
      <View
        style={[
          styles.content,
          {
            paddingTop: insets.top + Spacing["4xl"],
            paddingBottom: insets.bottom + Spacing["2xl"],
          },
        ]}
      >
        <View style={styles.header}>
          <View
            style={[
              styles.lockCircle,
              {
                backgroundColor: error
                  ? theme.backgroundDefault
                  : theme.primary + "15",
              },
            ]}
          >
            <Feather
              name="lock"
              size={32}
              color={error ? theme.text : theme.primary}
            />
          </View>
          <ThemedText style={[styles.title, { color: theme.text }]}>
            Enter PIN Code
          </ThemedText>
          <ThemedText
            style={[styles.subtitle, { color: theme.textSecondary }]}
          >
            Please enter your 4-digit PIN to confirm
          </ThemedText>
        </View>

        <Animated.View
          style={[
            styles.pinContainer,
            { transform: [{ translateX: shakeAnim }] },
          ]}
        >
          {Array(PIN_LENGTH)
            .fill(0)
            .map((_, index) => (
              <Animated.View
                key={index}
                style={[
                  styles.pinDot,
                  {
                    backgroundColor:
                      pin.length > index
                        ? error
                          ? "#EF4444"
                          : theme.primary
                        : theme.backgroundDefault,
                    borderColor: error ? "#EF4444" : theme.border,
                    transform: [{ scale: scaleAnims[index] }],
                  },
                ]}
              />
            ))}
        </Animated.View>

        {error && (
          <View style={styles.errorContainer}>
            <Feather name="alert-circle" size={16} color="#EF4444" />
            <ThemedText style={styles.errorText}>Incorrect PIN</ThemedText>
          </View>
        )}

        {/* Hidden TextInput for keyboard */}
        <TextInput
          style={styles.hiddenInput}
          value={pin}
          onChangeText={handlePinInput}
          keyboardType="number-pad"
          maxLength={PIN_LENGTH}
          autoFocus
          secureTextEntry
        />

        <Pressable
          onPress={() => router.back()}
          style={({ pressed }) => [
            styles.cancelButton,
            { opacity: pressed ? 0.7 : 1 },
          ]}
        >
          <ThemedText style={[styles.cancelText, { color: theme.textSecondary }]}>
            Cancel
          </ThemedText>
        </Pressable>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  content: {
    flex: 1,
    paddingHorizontal: Spacing.lg,
    justifyContent: "center",
    alignItems: "center",
  },
  header: {
    alignItems: "center",
    marginBottom: Spacing["4xl"],
  },
  lockCircle: {
    width: 80,
    height: 80,
    borderRadius: 40,
    alignItems: "center",
    justifyContent: "center",
    marginBottom: Spacing.xl,
  },
  title: {
    fontSize: 24,
    fontWeight: "700",
    marginBottom: Spacing.sm,
  },
  subtitle: {
    fontSize: 14,
    textAlign: "center",
  },
  pinContainer: {
    flexDirection: "row",
    gap: Spacing.lg,
    marginBottom: Spacing.xl,
  },
  pinDot: {
    width: 20,
    height: 20,
    borderRadius: 10,
    borderWidth: 2,
  },
  hiddenInput: {
    position: "absolute",
    opacity: 0,
    height: 0,
    width: 0,
  },
  errorContainer: {
    flexDirection: "row",
    alignItems: "center",
    gap: Spacing.xs,
    marginTop: Spacing.md,
  },
  errorText: {
    fontSize: 14,
    color: "#EF4444",
    fontWeight: "500",
  },
  cancelButton: {
    marginTop: Spacing["4xl"],
    paddingVertical: Spacing.md,
    paddingHorizontal: Spacing.xl,
  },
  cancelText: {
    fontSize: 16,
    fontWeight: "500",
  },
});
