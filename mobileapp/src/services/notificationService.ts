import { Platform } from "react-native";
import * as Notifications from "expo-notifications";
import * as Linking from "expo-linking";
import AsyncStorage from "@react-native-async-storage/async-storage";
import Constants from "expo-constants";

const NOTIFICATION_PREFERENCE_KEY = "blink_notifications_enabled";
const PUSH_TOKEN_KEY = "blink_push_token";
const API_BASE = process.env.EXPO_PUBLIC_API_URL || "https://api.blinks.app";

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

export async function saveNotificationPreference(value: boolean): Promise<void> {
  try {
    await AsyncStorage.setItem(NOTIFICATION_PREFERENCE_KEY, value ? "true" : "false");
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

async function sendDeviceTokenToBackend(token: string): Promise<void> {
  if (!token) {
    return;
  }

  try {
    await fetch(`${API_BASE}/notifications/register`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        token,
        platform: Platform.OS,
        appId:
          Constants.manifest?.slug || Constants.expoConfig?.slug || "BLINKS",
      }),
    });
  } catch {
    // Backend registration is optional during local development.
  }
}

export async function requestNotificationPermissionsAsync(): Promise<Notifications.PermissionStatus> {
  try {
    const current = await Notifications.getPermissionsAsync();
    if (current.granted || current.ios?.status === Notifications.PermissionStatus.PROVISIONAL) {
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

export async function registerForPushNotificationsAsync(): Promise<string | null> {
  if (!Constants.isDevice) {
    console.warn("Push notifications require a physical device.");
    return null;
  }

  const status = await requestNotificationPermissionsAsync();
  if (status !== Notifications.PermissionStatus.GRANTED) {
    return null;
  }

  try {
    const tokenResponse = await Notifications.getExpoPushTokenAsync();
    const token = typeof tokenResponse === "string" ? tokenResponse : tokenResponse.data;

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
