import "react-native-get-random-values";
import "react-native-url-polyfill/auto";
import React from "react";
import { Stack, useRouter, usePathname } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { View } from "react-native";
import { PrivyProvider } from "@privy-io/expo-sdk";
import { COLORS } from "../src/constants/colors";
import { useFonts } from "expo-font";
import { Anton_400Regular } from "@expo-google-fonts/anton";
import {
  Outfit_400Regular,
  Outfit_500Medium,
  Outfit_700Bold,
} from "@expo-google-fonts/outfit";
import { ErrorBoundary } from "../src/components/ErrorBoundary";
import { ToastManager } from "../src/components/Toast";
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
  useOfflineDetection();

  React.useEffect(() => {
    startNavigation(pathname);
    return () => {
      logNavigation(pathname);
    };
  }, [pathname]);

  React.useEffect(() => {
    async function setupNotifications() {
      await initNotificationCategoriesAsync();

      const enabled = await getStoredNotificationPreference();
      if (enabled) {
        await registerForPushNotificationsAsync();
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
      <ToastManager />
    </View>
  );
}

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
        <LayoutContent />
      </ErrorBoundary>
    </PrivyProvider>
  );
}