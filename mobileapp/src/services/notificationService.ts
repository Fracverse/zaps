import { Platform } from "react-native";
import * as Notifications from "expo-notifications";
import * as Linking from "expo-linking";
import AsyncStorage from "@react-native-async-storage/async-storage";
import Constants from "expo-constants";
import { apiFetch } from "./api";

const NOTIFICATION_PREFERENCE_KEY = "zaps_notifications_enabled";
const PUSH_TOKEN_KEY = "zaps_push_token";
const NOTIFICATION_PROMPT_DISMISSED_KEY = "zaps_notifications_prompt_dismissed";
const API_BASE = process.env.EXPO_PUBLIC_API_URL || "https://api.zaps.app";

export const NOTIFICATION_CATEGORIES = {
  TRANSACTION: "TRANSACTION",
};

export async function initNotificationCategoriesAsync(): Promise<void> {
  if (Platform.OS === "web") {
    return;
  }

  try {
    await Notifications.setNotificationCategoryAsync(
      NOTIFICATION_CATEGORIES.TRANSACTION,
      [
        {
          identifier: "VIEW_TRANSACTION",
          buttonTitle: "View",
          options: { opensAppToForeground: true },
        },
        {
          identifier: "DISMISS",
          buttonTitle: "Dismiss",
          options: { opensAppToForeground: false },
        },
        {
          identifier: "MARK_READ",
          buttonTitle: "Mark Read",
          options: { opensAppToForeground: false },
        },
      ]
    );
  } catch (error) {
    console.warn("Notification categories initialization failed", error);
  }
}

export async function getStoredNotificationPreference(): Promise<boolean> {
  try {
    const raw = await AsyncStorage.getItem(NOTIFICATION_PREFERENCE_KEY);
    return raw !== "false";
  } catch {
    return true;
  }
}

export async function saveNotificationPreference(
  value: boolean
): Promise<void> {
  try {
    await AsyncStorage.setItem(
      NOTIFICATION_PREFERENCE_KEY,
      value ? "true" : "false"
    );
  } catch {
    // ignore write failures
  }
}

export async function getStoredPushToken(): Promise<string | null> {
  try {
    return await AsyncStorage.getItem(PUSH_TOKEN_KEY);
  } catch {
    return null;
  }
}

export async function removeStoredPushToken(): Promise<void> {
  try {
    await AsyncStorage.removeItem(PUSH_TOKEN_KEY);
  } catch {
    // ignore remove failures
  }
}

/** Whether the user has already dismissed the in-app "enable notifications" prompt. */
export async function getHasDismissedNotificationPrompt(): Promise<boolean> {
  try {
    return (
      (await AsyncStorage.getItem(NOTIFICATION_PROMPT_DISMISSED_KEY)) === "true"
    );
  } catch {
    return false;
  }
}

/** Records that the user tapped "Not now" (or "Enable") so we don't nag again. */
export async function markNotificationPromptDismissed(): Promise<void> {
  try {
    await AsyncStorage.setItem(NOTIFICATION_PROMPT_DISMISSED_KEY, "true");
  } catch {
    // ignore write failures
  }
}

/**
 * Gate for the Home screen's soft-ask notification banner: only show it when
 * permission genuinely hasn't been decided yet and the user hasn't already
 * dismissed it once. Physical device + native-only, same constraints as
 * `registerForPushNotificationsAsync`.
 */
export async function shouldShowNotificationConsentPrompt(): Promise<boolean> {
  if (Platform.OS === "web" || !Constants.isDevice) {
    return false;
  }

  try {
    const [current, dismissed] = await Promise.all([
      Notifications.getPermissionsAsync(),
      getHasDismissedNotificationPrompt(),
    ]);
    return (
      current.status === Notifications.PermissionStatus.UNDETERMINED &&
      !dismissed
    );
  } catch {
    return false;
  }
}

async function sendDeviceTokenToBackend(token: string): Promise<boolean> {
  if (!token) {
    return false;
  }

  try {
    // Authenticated via apiFetch (Bearer token) so the backend can associate
    // this Expo push token with the signed-in user rather than just a device.
    const response = await apiFetch(`${API_BASE}/api/notifications/register`, {
      method: "POST",
      body: JSON.stringify({
        token,
        platform: Platform.OS,
        appId: Constants.manifest?.slug || Constants.expoConfig?.slug || "ZAPS",
      }),
    });
    return response.ok;
  } catch (error) {
    // Local dev / flaky network shouldn't block the rest of the app —
    // the token stays cached locally and a later app start will retry.
    console.warn("sendDeviceTokenToBackend error", error);
    return false;
  }
}

export async function requestNotificationPermissionsAsync(): Promise<Notifications.PermissionStatus> {
  try {
    const current = await Notifications.getPermissionsAsync();
    if (
      current.granted ||
      current.ios?.status === Notifications.PermissionStatus.PROVISIONAL
    ) {
      return current.status;
    }

    const permission = await Notifications.requestPermissionsAsync({
      ios: {
        allowAlert: true,
        allowSound: true,
        allowBadge: true,
      },
      android: {
        allowAlert: true,
        allowSound: true,
        allowVibrate: true,
      },
    });

    return permission.status;
  } catch (error) {
    console.warn("requestNotificationPermissionsAsync error", error);
    return Notifications.PermissionStatus.UNDETERMINED;
  }
}

export async function registerForPushNotificationsAsync(
  options: { requestIfUndetermined?: boolean } = {}
): Promise<string | null> {
  const { requestIfUndetermined = true } = options;

  if (!Constants.isDevice) {
    console.warn("Push notifications require a physical device.");
    return null;
  }

  // By default this may trigger the native OS permission dialog. Callers
  // that only want to silently refresh a token the user has already
  // consented to (e.g. app boot) should pass `requestIfUndetermined: false`
  // so an undetermined status is left untouched rather than re-prompted.
  const status = requestIfUndetermined
    ? await requestNotificationPermissionsAsync()
    : (await Notifications.getPermissionsAsync()).status;

  if (status !== Notifications.PermissionStatus.GRANTED) {
    return null;
  }

  try {
    const tokenResponse = await Notifications.getExpoPushTokenAsync();
    const token =
      typeof tokenResponse === "string" ? tokenResponse : tokenResponse.data;

    if (!token) {
      return null;
    }

    await AsyncStorage.setItem(PUSH_TOKEN_KEY, token);
    await sendDeviceTokenToBackend(token);
    return token;
  } catch (error) {
    console.warn("registerForPushNotificationsAsync error", error);
    return null;
  }
}

export function getNotificationDeepLink(data: any): string | null {
  if (!data) {
    return null;
  }

  if (typeof data.url === "string" && data.url.length) {
    return data.url;
  }

  if (data.target === "transaction" && typeof data.transactionId === "string") {
    return `/transaction/${data.transactionId}`;
  }

  if (data.target === "payment" && typeof data.paymentId === "string") {
    return `/transaction/${data.paymentId}`;
  }

  if (data.target === "merchantPayment") {
    return "/merchant/payment-received";
  }

  if (data.target === "home") {
    return "/(personal)/home";
  }

  return null;
}

export async function handleNotificationResponse(
  response: Notifications.NotificationResponse,
  router: { push: (path: string) => void }
): Promise<void> {
  try {
    const data = response.notification.request.content.data as any;
    const deepLink = getNotificationDeepLink(data);

    if (deepLink) {
      router.push(deepLink);
      return;
    }

    if (typeof data?.url === "string" && data.url.startsWith("http")) {
      await Linking.openURL(data.url);
      return;
    }

    if (typeof data?.transactionId === "string") {
      router.push(`/transaction/${data.transactionId}`);
      return;
    }

    router.push("/(personal)/home");
  } catch (error) {
    console.warn("handleNotificationResponse error", error);
  }
}
