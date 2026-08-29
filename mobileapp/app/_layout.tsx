import "react-native-get-random-values";
import "react-native-url-polyfill/auto";
import React from "react";
import { Stack, useRouter, usePathname } from "expo-router";
import { StatusBar } from "expo-status-bar";
import {
  Linking as RNLinking,
  Modal,
  Platform,
  StyleSheet,
  Text,
  TouchableOpacity,
  View,
} from "react-native";
import { PrivyProvider } from "@privy-io/expo-sdk";
import Constants from "expo-constants";
import { COLORS } from "../src/constants/colors";
import { useFonts } from "expo-font";
import { Anton_400Regular } from "@expo-google-fonts/anton";
import {
  Outfit_400Regular,
  Outfit_500Medium,
  Outfit_700Bold,
} from "@expo-google-fonts/outfit";
import { ErrorBoundary } from "../src/components/ErrorBoundary";
import { ToastProvider } from "../src/components/Toast";
import { useOfflineDetection } from "../src/hooks/useNetworkStatus";
import {
  getStoredNotificationPreference,
  handleNotificationResponse,
  initNotificationCategoriesAsync,
  registerForPushNotificationsAsync,
} from "../src/services/notificationService";
import * as Notifications from "expo-notifications";
import "../src/locales/i18n"; // Initialize i18n
import { logNavigation, startNavigation } from "../src/utils/performance";
import * as Linking from "expo-linking";
import { parseSdpClaimUrl } from "../src/utils/sdpDeepLink";
import { initOfflineSync } from "../src/services/api";

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";
const IOS_STORE_URL = "https://apps.apple.com/app/zaps";
const ANDROID_STORE_URL =
  "https://play.google.com/store/apps/details?id=app.zaps";

type AppConfigResponse = {
  minimum_required_version?: string;
  min_version?: string;
  ios_store_url?: string;
  android_store_url?: string;
};

/** Compare dotted version strings (app.json `expo.version` vs backend min). */
function compareVersions(installed: string, required: string): number {
  const left = installed.split(".").map((part) => parseInt(part, 10) || 0);
  const right = required.split(".").map((part) => parseInt(part, 10) || 0);
  const len = Math.max(left.length, right.length);
  for (let i = 0; i < len; i += 1) {
    const a = left[i] ?? 0;
    const b = right[i] ?? 0;
    if (a < b) return -1;
    if (a > b) return 1;
  }
  return 0;
}

Notifications.setNotificationHandler({
  handleNotification: async () => ({
    shouldShowAlert: true,
    shouldPlaySound: true,
    shouldSetBadge: true,
  }),
});

function LayoutContent() {
  const router = useRouter();
  const pathname = usePathname();
  const [forceUpdate, setForceUpdate] = React.useState(false);
  const [storeUrl, setStoreUrl] = React.useState(
    Platform.OS === "ios" ? IOS_STORE_URL : ANDROID_STORE_URL
  );
  useOfflineDetection();

  // #805 — Block the app when the installed app.json version is below
  // the backend minimum required version from GET /api/v1/config.
  React.useEffect(() => {
    let cancelled = false;

    async function checkMinimumVersion() {
      try {
        const response = await fetch(`${API_BASE}/api/v1/config`);
        if (!response.ok) return;

        const config = (await response.json()) as AppConfigResponse;
        const required =
          config.minimum_required_version ?? config.min_version ?? "";
        if (!required) return;

        const installed = Constants.expoConfig?.version ?? "0.0.0";
        if (compareVersions(installed, required) >= 0) return;
        if (cancelled) return;

        const nextStoreUrl =
          Platform.OS === "ios"
            ? config.ios_store_url || IOS_STORE_URL
            : config.android_store_url || ANDROID_STORE_URL;
        setStoreUrl(nextStoreUrl);
        setForceUpdate(true);
      } catch {
        // Fail open: a config outage should not brick the app.
      }
    }

    checkMinimumVersion();
    return () => {
      cancelled = true;
    };
  }, []);

  React.useEffect(() => {
    startNavigation(pathname);
    return () => {
      logNavigation(pathname);
    };
  }, [pathname]);

  // #687 — Start offline queue sync listener on app launch.
  React.useEffect(() => {
    const cleanup = initOfflineSync();
    return cleanup;
  }, []);

  React.useEffect(() => {
    async function setupNotifications() {
      await initNotificationCategoriesAsync();

      const enabled = await getStoredNotificationPreference();
      if (enabled) {
        // Cold start must never surface the native permission dialog itself —
        // first-time consent is asked contextually from the Home screen. This
        // only silently refreshes the Expo push token for a user who has
        // already granted permission in a previous session.
        await registerForPushNotificationsAsync({ requestIfUndetermined: false });
      }
    }

    setupNotifications();
  }, []);

  // Handle SDP claiming invite deep links — cold start (getInitialURL) and
  // warm start (url event) both funnel through the same parser/navigation.
  const handleIncomingUrl = React.useCallback(
    (url: string | null) => {
      if (!url) return;

      const result = parseSdpClaimUrl(url);
      if (!result.valid) {
        // Not every deep link is an SDP claim link (or it's malformed) —
        // ignore silently rather than disrupting normal navigation.
        return;
      }

      router.push(`/claim/${result.token}`);
    },
    [router]
  );

  React.useEffect(() => {
    // Cold start: app was launched directly from the deep link.
    Linking.getInitialURL().then(handleIncomingUrl);

    // Warm start: app was already running in the background.
    const subscription = Linking.addEventListener("url", ({ url }) =>
      handleIncomingUrl(url)
    );

    return () => {
      subscription.remove();
    };
  }, [handleIncomingUrl]);

  React.useEffect(() => {
    const receivedListener = Notifications.addNotificationReceivedListener(
      () => {
        // Receipt can be used for analytics or local display enhancements.
      }
    );

    const responseListener =
      Notifications.addNotificationResponseReceivedListener((response) => {
        handleNotificationResponse(response, router);
      });

    return () => {
      receivedListener.remove();
      responseListener.remove();
    };
  }, [router]);

  return (
    <View style={{ flex: 1 }}>
      <StatusBar style="auto" />
      <Stack
        screenOptions={{
          headerShown: false,
          contentStyle: { backgroundColor: COLORS.white },
        }}
      >
        {/* Existing screens */}
        <Stack.Screen name="index" />
        <Stack.Screen name="onboarding-start" />
        <Stack.Screen name="account-type/index" />
        <Stack.Screen name="create-wallet" />
        <Stack.Screen name="backup-key" />
        <Stack.Screen name="password" />
        <Stack.Screen name="biometric" />
        <Stack.Screen name="username" />

        {/* Secure key management screens — Issue #97 */}
        <Stack.Screen
          name="mnemonic-backup"
          options={{
            // Prevent swipe-back while the phrase is visible
            gestureEnabled: false,
            animation: "slide_from_right",
          }}
        />
        <Stack.Screen
          name="wallet-recovery"
          options={{
            gestureEnabled: true,
            animation: "slide_from_right",
          }}
        />

        {/* SDP claiming invite validation — Issue #578 */}
        <Stack.Screen
          name="claim/[token]"
          options={{ animation: "slide_from_right" }}
        />

        {/* Non-critical info screens — deferred animation for faster perceived load */}
        <Stack.Screen name="faq" options={{ animation: "fade" }} />
        <Stack.Screen name="terms-of-service" options={{ animation: "fade" }} />
        <Stack.Screen name="privacy-policy" options={{ animation: "fade" }} />
        <Stack.Screen name="about-zaps" options={{ animation: "fade" }} />
        <Stack.Screen name="help-support" options={{ animation: "fade" }} />
      </Stack>
      <Modal
        visible={forceUpdate}
        transparent
        animationType="fade"
        // Non-dismissible: hardware back / Android request-close is ignored.
        onRequestClose={() => {}}
      >
        <View style={updateStyles.overlay}>
          <View style={updateStyles.card}>
            <Text style={updateStyles.title}>Update required</Text>
            <Text style={updateStyles.body}>
              A newer version of ZAPS is required to continue. Please update
              from the store to keep using the app.
            </Text>
            <TouchableOpacity
              style={updateStyles.button}
              onPress={() => RNLinking.openURL(storeUrl)}
              accessibilityRole="link"
              accessibilityLabel="Update now"
            >
              <Text style={updateStyles.buttonText}>Update now</Text>
            </TouchableOpacity>
          </View>
        </View>
      </Modal>
    </View>
  );
}

const updateStyles = StyleSheet.create({
  overlay: {
    flex: 1,
    backgroundColor: "rgba(0, 0, 0, 0.72)",
    alignItems: "center",
    justifyContent: "center",
    padding: 24,
  },
  card: {
    width: "100%",
    maxWidth: 360,
    borderRadius: 16,
    backgroundColor: COLORS.white,
    padding: 24,
  },
  title: {
    fontFamily: "Outfit_700Bold",
    fontSize: 22,
    color: COLORS.primary,
    marginBottom: 8,
  },
  body: {
    fontFamily: "Outfit_400Regular",
    fontSize: 15,
    lineHeight: 22,
    color: COLORS.darkGray,
    marginBottom: 20,
  },
  button: {
    backgroundColor: COLORS.primary,
    borderRadius: 12,
    paddingVertical: 14,
    alignItems: "center",
  },
  buttonText: {
    fontFamily: "Outfit_700Bold",
    fontSize: 16,
    color: COLORS.secondary,
  },
});

export default function Layout() {
  const [fontsLoaded] = useFonts({
    Anton_400Regular,
    Outfit_400Regular,
    Outfit_500Medium,
    Outfit_700Bold,
  });

  if (!fontsLoaded) {
    return null;
  }

  return (
    <PrivyProvider
      appId={process.env.EXPO_PUBLIC_PRIVY_APP_ID || ""}
      config={{
        loginMethods: ["google", "apple", "email"],
        appearance: { theme: "light" },
      }}
    >
      <ErrorBoundary>
        <ToastProvider>
          <LayoutContent />
        </ToastProvider>
      </ErrorBoundary>
    </PrivyProvider>
  );
}