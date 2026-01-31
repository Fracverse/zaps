import React, { useEffect, useRef } from "react";
import { View, StyleSheet, Pressable, Animated, Easing } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { Feather } from "@expo/vector-icons";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";

import { ThemedText } from "@/src/components/ThemedText";
import { useTheme } from "@/src/hooks/useTheme";
import { Spacing, BorderRadius } from "@/src/constants/theme";

export default function SuccessScreen() {
  const insets = useSafeAreaInsets();
  const { theme } = useTheme();
  const router = useRouter();

  const scaleAnim = useRef(new Animated.Value(0)).current;
  const checkAnim = useRef(new Animated.Value(0)).current;
  const fadeAnim = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);

    // Success animation sequence
    Animated.sequence([
      Animated.parallel([
        Animated.spring(scaleAnim, {
          toValue: 1,
          tension: 50,
          friction: 7,
          useNativeDriver: true,
        }),
        Animated.timing(fadeAnim, {
          toValue: 1,
          duration: 300,
          useNativeDriver: true,
        }),
      ]),
      Animated.timing(checkAnim, {
        toValue: 1,
        duration: 400,
        easing: Easing.bezier(0.25, 0.1, 0.25, 1),
        useNativeDriver: true,
      }),
    ]).start();
  }, [scaleAnim, fadeAnim, checkAnim]);

  const formatCurrency = (value: number) => {
    return value.toLocaleString("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  };

  const handleDone = () => {
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
    router.push("/");
  };

  const checkScale = checkAnim.interpolate({
    inputRange: [0, 0.5, 1],
    outputRange: [0, 1.2, 1],
  });

  return (
    <View style={[styles.container, { backgroundColor: theme.backgroundRoot }]}>
      <View
        style={[
          styles.content,
          {
            paddingTop: insets.top,
            paddingBottom: insets.bottom + Spacing.lg,
          },
        ]}
      >
        <View style={styles.topSection}>
          <Animated.View
            style={[
              styles.successIconOuter,
              {
                transform: [{ scale: scaleAnim }],
                opacity: fadeAnim,
              },
            ]}
          >
            <View style={styles.successIconInner}>
              <Animated.View
                style={{
                  transform: [{ scale: checkScale }],
                }}
              >
                <Feather name="check" size={60} color="#1A4B4A" />
              </Animated.View>
            </View>
          </Animated.View>

          <Animated.View style={[styles.textSection, { opacity: fadeAnim }]}>
            <ThemedText style={styles.title}>Withdrawal Successful</ThemedText>

            <View style={styles.amountCapsule}>
              <ThemedText style={styles.amountText}>
                ${formatCurrency(15046.12)}
              </ThemedText>
            </View>
          </Animated.View>
        </View>

        <Animated.View style={[styles.buttonContainer, { opacity: fadeAnim }]}>
          <Pressable
            onPress={handleDone}
            style={({ pressed }) => [
              styles.primaryButton,
              {
                backgroundColor: theme.primary,
                opacity: pressed ? 0.9 : 1,
              },
            ]}
          >
            <ThemedText style={styles.primaryButtonText}>Done</ThemedText>
          </Pressable>
        </Animated.View>
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
    justifyContent: "space-between",
  },
  topSection: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    gap: Spacing["4xl"],
  },
  successIconOuter: {
    width: 140,
    height: 140,
    borderRadius: 70,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    alignItems: "center",
    justifyContent: "center",
  },
  successIconInner: {
    width: 100,
    height: 100,
    borderRadius: 50,
    borderWidth: 2,
    borderColor: "#1A4B4A",
    alignItems: "center",
    justifyContent: "center",
  },
  textSection: {
    alignItems: "center",
    gap: Spacing.xl,
  },
  title: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    textAlign: "center",
  },
  amountCapsule: {
    paddingHorizontal: Spacing["4xl"],
    paddingVertical: Spacing.md,
    borderRadius: BorderRadius.full,
    borderWidth: 1,
    borderColor: "#000000",
  },
  amountText: {
    fontSize: 34,
    fontFamily: "Outfit_700Bold",
  },
  buttonContainer: {
    width: "100%",
  },
  primaryButton: {
    height: Spacing.buttonHeight,
    borderRadius: BorderRadius.full,
    alignItems: "center",
    justifyContent: "center",
  },
  primaryButtonText: {
    color: "#8CF79C",
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
  },
});
