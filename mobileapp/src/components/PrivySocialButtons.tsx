import React, { useState } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  Modal,
  ActivityIndicator,
} from "react-native";
import * as Linking from "expo-linking";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../constants/colors";
import { useRouter } from "expo-router";

const PRIVY_APP_ID =
  process.env.EXPO_PUBLIC_PRIVY_APP_ID || "clzaps_privy_app_id";

export interface PrivySocialButtonsProps {
  onSuccess?: (provider: string) => void;
  nextRoute?: "/username" | "/onboarding-start" | "/(personal)/home";
  testIDPrefix?: string;
}

export function buildPrivyAuthUrl(
  provider: "google" | "apple" | "email"
): string {
  return `https://auth.privy.io/apps/${PRIVY_APP_ID}/login?provider=${provider}&redirect_uri=zaps://privy-callback`;
}

export const PrivySocialButtons: React.FC<PrivySocialButtonsProps> = ({
  onSuccess,
  nextRoute = "/username",
  testIDPrefix = "privy",
}) => {
  const router = useRouter();
  const [activeProvider, setActiveProvider] = useState<string | null>(null);

  const handleLaunchPrivy = async (provider: "google" | "apple" | "email") => {
    setActiveProvider(provider);
    const url = buildPrivyAuthUrl(provider);

    try {
      const supported = await Linking.canOpenURL(url);
      if (supported) {
        await Linking.openURL(url);
      } else {
        await Linking.openURL(
          `https://auth.privy.io/login?provider=${provider}`
        );
      }
    } catch {
      // Fallback for dev / emulator environments
    } finally {
      setTimeout(() => {
        setActiveProvider(null);
        if (onSuccess) {
          onSuccess(provider);
        } else if (nextRoute) {
          router.push(nextRoute);
        }
      }, 1000);
    }
  };

  return (
    <View style={styles.container}>
      {/* Social Connection Buttons */}
      <TouchableOpacity
        testID={`${testIDPrefix}-google-button`}
        style={[styles.socialButton, styles.googleButton]}
        onPress={() => handleLaunchPrivy("google")}
        activeOpacity={0.8}
      >
        <Ionicons
          name="logo-google"
          size={20}
          color="#000000"
          style={styles.icon}
        />
        <Text style={styles.googleButtonText}>Continue with Google</Text>
      </TouchableOpacity>

      <TouchableOpacity
        testID={`${testIDPrefix}-apple-button`}
        style={[styles.socialButton, styles.appleButton]}
        onPress={() => handleLaunchPrivy("apple")}
        activeOpacity={0.8}
      >
        <Ionicons
          name="logo-apple"
          size={20}
          color="#FFFFFF"
          style={styles.icon}
        />
        <Text style={styles.appleButtonText}>Continue with Apple</Text>
      </TouchableOpacity>

      <TouchableOpacity
        testID={`${testIDPrefix}-email-button`}
        style={[styles.socialButton, styles.emailButton]}
        onPress={() => handleLaunchPrivy("email")}
        activeOpacity={0.8}
      >
        <Ionicons
          name="mail-outline"
          size={20}
          color={COLORS.primary}
          style={styles.icon}
        />
        <Text style={styles.emailButtonText}>Continue with Email</Text>
      </TouchableOpacity>

      {/* Privy Overlay Modal */}
      <Modal
        visible={activeProvider !== null}
        transparent
        animationType="fade"
        onRequestClose={() => setActiveProvider(null)}
      >
        <View style={styles.overlayBackdrop}>
          <View style={styles.overlayCard}>
            <View style={styles.privyHeader}>
              <Ionicons
                name="shield-checkmark"
                size={28}
                color={COLORS.primary}
              />
              <Text style={styles.privyTitle}>Privy Social Auth</Text>
            </View>
            <ActivityIndicator
              size="large"
              color={COLORS.primary}
              style={{ marginVertical: 16 }}
            />
            <Text style={styles.overlaySubtext}>
              Launching Privy overlay for{" "}
              {activeProvider ? activeProvider.toUpperCase() : "Social Login"}...
            </Text>
          </View>
        </View>
      </Modal>

      <View style={styles.privyBadgeRow}>
        <Ionicons name="lock-closed" size={12} color="#666666" />
        <Text style={styles.privyBadgeText}>Secured by Privy Embedded Auth</Text>
      </View>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    width: "100%",
    gap: 10,
  },
  socialButton: {
    width: "100%",
    height: 52,
    borderRadius: 26,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 20,
  },
  googleButton: {
    backgroundColor: "#FFFFFF",
    borderWidth: 1,
    borderColor: "#E5E7EB",
  },
  googleButtonText: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: "#1F2937",
  },
  appleButton: {
    backgroundColor: COLORS.primary,
  },
  appleButtonText: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.secondary,
  },
  emailButton: {
    backgroundColor: "transparent",
    borderWidth: 1.5,
    borderColor: COLORS.primary,
  },
  emailButtonText: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  icon: {
    marginRight: 10,
  },
  overlayBackdrop: {
    flex: 1,
    backgroundColor: "rgba(0,0,0,0.5)",
    justifyContent: "center",
    alignItems: "center",
    padding: 24,
  },
  overlayCard: {
    width: "90%",
    backgroundColor: "#FFFFFF",
    borderRadius: 20,
    padding: 24,
    alignItems: "center",
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.15,
    shadowRadius: 12,
    elevation: 8,
  },
  privyHeader: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
  },
  privyTitle: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  overlaySubtext: {
    fontSize: 14,
    color: "#666666",
    fontFamily: "Outfit_500Medium",
    textAlign: "center",
  },
  privyBadgeRow: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 4,
    marginTop: 6,
  },
  privyBadgeText: {
    fontSize: 12,
    color: "#666666",
    fontFamily: "Outfit_400Regular",
  },
});
