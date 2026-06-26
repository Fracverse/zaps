import { useState, useCallback } from "react";
import { useCameraPermissions, PermissionStatus } from "expo-camera";

export interface CameraPermissionState {
  granted: boolean;
  denied: boolean;
  undetermined: boolean;
  loading: boolean;
  requestPermission: () => Promise<boolean>;
}

export function useCameraPermission(): CameraPermissionState {
  const [permission, requestExpoPermission] = useCameraPermissions();
  const [loading, setLoading] = useState(false);

  const granted = permission?.status === PermissionStatus.GRANTED;
  const denied = permission?.status === PermissionStatus.DENIED;
  const undetermined =
    !permission || permission.status === PermissionStatus.UNDETERMINED;

  const requestPermission = useCallback(async (): Promise<boolean> => {
    setLoading(true);
    try {
      const result = await requestExpoPermission();
      return result.granted;
    } finally {
      setLoading(false);
    }
  }, [requestExpoPermission]);

  return { granted, denied, undetermined, loading, requestPermission };
}
