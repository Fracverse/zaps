import { useCallback, useEffect, useState } from "react";
import * as Notifications from "expo-notifications";
import * as Linking from "expo-linking";
import {
  getStoredNotificationPreference,
  removeStoredPushToken,
  registerForPushNotificationsAsync,
  requestNotificationPermissionsAsync,
  saveNotificationPreference,
} from "../services/notificationService";

export function useNotificationPreferences() {
  const [enabled, setEnabled] = useState<boolean>(true);
  const [permissionStatus, setPermissionStatus] = useState<Notifications.PermissionStatus>(
    Notifications.PermissionStatus.UNDETERMINED
  );
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function loadPreferences() {
      const stored = await getStoredNotificationPreference();
      const current = await Notifications.getPermissionsAsync();
      setEnabled(stored);
      setPermissionStatus(current.status);
      setLoading(false);
    }

    loadPreferences();
  }, []);

  const toggleNotifications = useCallback(async (value: boolean) => {
    await saveNotificationPreference(value);
    setEnabled(value);

    if (value) {
      const status = await requestNotificationPermissionsAsync();
      setPermissionStatus(status);
      if (status === Notifications.PermissionStatus.GRANTED) {
        await registerForPushNotificationsAsync();
      } else if (status === Notifications.PermissionStatus.DENIED) {
        await Linking.openSettings();
      }
    } else {
      await removeStoredPushToken();
      const current = await Notifications.getPermissionsAsync();
      setPermissionStatus(current.status);
    }
  }, []);

  const openSystemSettings = useCallback(async () => {
    await Linking.openSettings();
  }, []);

  return {
    enabled,
    permissionStatus,
    loading,
    toggleNotifications,
    openSystemSettings,
  };
}
