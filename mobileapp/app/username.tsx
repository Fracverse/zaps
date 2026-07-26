import React, { useState, useEffect, useRef } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ActivityIndicator,
  KeyboardAvoidingView,
  Platform,
  TouchableWithoutFeedback,
  Keyboard,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import AsyncStorage from "@react-native-async-storage/async-storage";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Input } from "../src/components/Input";
import { Ionicons } from "@expo/vector-icons";
import { fetchWithRetry } from "../src/utils/retry";

const API_BASE = process.env.EXPO_PUBLIC_API_URL || "http://localhost:8080";
export const USERNAME_STORAGE_KEY = "user_username";

export const MIN_USERNAME_LENGTH = 3;
export const MAX_USERNAME_LENGTH = 20;

export interface ValidationResult {
  isValid: boolean;
  error?: string;
}

export function validateUsernameFormat(username: string): ValidationResult {
  if (!username) {
    return { isValid: false };
  }
  if (username.length < MIN_USERNAME_LENGTH) {
    return {
      isValid: false,
      error: `Username must be at least ${MIN_USERNAME_LENGTH} characters`,
    };
  }
  if (username.length > MAX_USERNAME_LENGTH) {
    return {
      isValid: false,
      error: `Username must be at most ${MAX_USERNAME_LENGTH} characters`,
    };
  }
  const regex = /^[a-zA-Z0-9_]+$/;
  if (!regex.test(username)) {
    return {
      isValid: false,
      error: "Username can only contain letters, numbers, and underscores",
    };
  }
  return { isValid: true };
}

export async function checkUsernameAvailabilityApi(
  username: string
): Promise<boolean> {
  try {
    const res = await fetchWithRetry(
      `${API_BASE}/api/users/resolve/${encodeURIComponent(username)}`,
      { method: "GET" }
    );
    if (res.status === 404) {
      // 404 means user not found -> username is available
      return true;
    }
    if (res.ok) {
      // 200 means user exists -> username is taken
      return false;
    }
    return true;
  } catch {
    // If backend is unreachable or offline during dev/testing, default to available
    return true;
  }
}

export default function UsernameScreen() {
  const router = useRouter();
  const [username, setUsername] = useState("");
  const [formatError, setFormatError] = useState<string | undefined>();
  const [isChecking, setIsChecking] = useState(false);
  const [isAvailable, setIsAvailable] = useState<boolean | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const debounceTimerRef = useRef<NodeJS.Timeout | null>(null);

  const handleChangeText = (text: string) => {
    // Strip leading '@' if user typed or pasted it
    const cleaned = text.replace(/^@/, "");
    setUsername(cleaned);

    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
    }

    if (!cleaned) {
      setFormatError(undefined);
      setIsChecking(false);
      setIsAvailable(null);
      return;
    }

    const validation = validateUsernameFormat(cleaned);
    if (!validation.isValid) {
      setFormatError(validation.error);
      setIsChecking(false);
      setIsAvailable(null);
      return;
    }

    setFormatError(undefined);
    setIsChecking(true);
    setIsAvailable(null);

    debounceTimerRef.current = setTimeout(async () => {
      const available = await checkUsernameAvailabilityApi(cleaned);
      setIsAvailable(available);
      setIsChecking(false);
    }, 350);
  };

  useEffect(() => {
    return () => {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
    };
  }, []);

  const canContinue =
    username.trim().length >= MIN_USERNAME_LENGTH &&
    !formatError &&
    !isChecking &&
    isAvailable === true &&
    !submitting;

  const handleContinue = async () => {
    if (!canContinue) return;
    setSubmitting(true);
    try {
      const cleanName = username.trim();
      await AsyncStorage.setItem(USERNAME_STORAGE_KEY, cleanName);

      // Also merge into cached user profile if exists
      const existingProfile = await AsyncStorage.getItem("user_profile");
      if (existingProfile) {
        const parsed = JSON.parse(existingProfile);
        parsed.username = cleanName;
        await AsyncStorage.setItem("user_profile", JSON.stringify(parsed));
      } else {
        await AsyncStorage.setItem(
          "user_profile",
          JSON.stringify({ username: cleanName })
        );
      }

      // Move to dashboard on completion
      router.replace("/(personal)/home");
    } catch {
      // Fallback navigation
      router.replace("/(personal)/home");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />
      <TouchableWithoutFeedback onPress={Keyboard.dismiss}>
        <KeyboardAvoidingView
          style={{ flex: 1 }}
          behavior={Platform.OS === "ios" ? "padding" : "height"}
        >
          <View style={styles.header}>
            <TouchableOpacity
              style={styles.backButton}
              onPress={() => router.back()}
              testID="username-back-button"
            >
              <Ionicons name="arrow-back" size={24} color={COLORS.black} />
            </TouchableOpacity>
            <Text style={styles.headerTitle}>Select Username</Text>
            <View style={{ width: 24 }} />
          </View>

          <View style={styles.content}>
            <Text style={styles.title}>Choose your handle</Text>
            <Text style={styles.subtitle}>
              Your Zaps username is your unique ID for sending, receiving, and sharing social payments.
            </Text>

            <View style={styles.inputContainer}>
              <View style={styles.inputWrapper}>
                <Text style={styles.atSymbol}>@</Text>
                <Input
                  testID="username-input"
                  placeholder="username"
                  value={username}
                  onChangeText={handleChangeText}
                  autoCapitalize="none"
                  autoCorrect={false}
                  maxLength={MAX_USERNAME_LENGTH}
                  style={styles.customInput}
                  error={formatError}
                />
              </View>

              {/* Status Feedback Row */}
              {isChecking && (
                <View style={styles.statusRow} testID="status-checking">
                  <ActivityIndicator size="small" color={COLORS.primary} />
                  <Text style={styles.checkingText}>Checking availability...</Text>
                </View>
              )}

              {!isChecking && isAvailable === true && (
                <View style={styles.statusRow} testID="status-available">
                  <Ionicons
                    name="checkmark-circle"
                    size={18}
                    color="#22C55E"
                  />
                  <Text style={styles.availableText}>
                    @{username} is available!
                  </Text>
                </View>
              )}

              {!isChecking && isAvailable === false && (
                <View style={styles.statusRow} testID="status-taken">
                  <Ionicons name="close-circle" size={18} color="#EF4444" />
                  <Text style={styles.takenText}>
                    @{username} is already taken
                  </Text>
                </View>
              )}
            </View>
          </View>

          <View style={styles.footer}>
            <Button
              testID="username-continue-button"
              title={submitting ? "Saving..." : "Continue"}
              onPress={handleContinue}
              variant="primary"
              disabled={!canContinue}
              loading={submitting}
              style={canContinue ? styles.activeButton : styles.disabledButton}
            />
          </View>
        </KeyboardAvoidingView>
      </TouchableWithoutFeedback>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.white,
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 10,
  },
  backButton: {
    padding: 8,
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  content: {
    flex: 1,
    paddingHorizontal: 20,
    paddingTop: 20,
  },
  title: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 8,
  },
  subtitle: {
    fontSize: 15,
    color: "#666",
    marginBottom: 28,
    fontFamily: "Outfit_500Medium",
    lineHeight: 22,
  },
  inputContainer: {
    width: "100%",
  },
  inputWrapper: {
    position: "relative",
    width: "100%",
  },
  atSymbol: {
    position: "absolute",
    left: 20,
    top: 16,
    zIndex: 1,
    fontSize: 18,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
  },
  customInput: {
    paddingLeft: 42,
  },
  statusRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
    marginTop: 4,
    marginLeft: 12,
  },
  checkingText: {
    fontSize: 13,
    color: COLORS.primary,
    fontFamily: "Outfit_500Medium",
  },
  availableText: {
    fontSize: 13,
    color: "#22C55E",
    fontFamily: "Outfit_500Medium",
  },
  takenText: {
    fontSize: 13,
    color: "#EF4444",
    fontFamily: "Outfit_500Medium",
  },
  footer: {
    padding: 20,
    paddingBottom: Platform.OS === "ios" ? 30 : 20,
  },
  activeButton: {
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    height: 56,
  },
  disabledButton: {
    opacity: 0.5,
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    height: 56,
  },
});

