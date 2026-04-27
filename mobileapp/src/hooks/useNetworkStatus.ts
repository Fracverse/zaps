import { useState, useEffect } from "react";
import NetInfo from "@react-native-community/netinfo";

interface NetworkStatus {
  isConnected: boolean | null;
  isInternetReachable: boolean | null;
  type: string;
  details?: any;
}

export const useNetworkStatus = (): NetworkStatus => {
  const [networkStatus, setNetworkStatus] = useState<NetworkStatus>({
    isConnected: null, // Unknown initially
    isInternetReachable: null,
    type: "unknown",
  });

  useEffect(() => {
    const unsubscribe = NetInfo.addEventListener((state) => {
      setNetworkStatus({
        isConnected: state.isConnected,
        isInternetReachable: state.isInternetReachable,
        type: state.type,
        details: state.details,
      });
    });

    // Initial check
    NetInfo.fetch().then((state) => {
      setNetworkStatus({
        isConnected: state.isConnected,
        isInternetReachable: state.isInternetReachable,
        type: state.type,
        details: state.details,
      });
    });

    return () => unsubscribe();
  }, []);

  return networkStatus;
};

// Hook for offline detection with toast notifications
export const useOfflineDetection = () => {
  const networkStatus = useNetworkStatus();
  const [wasOffline, setWasOffline] = useState(false);

  useEffect(() => {
    if (networkStatus.isConnected === false) {
      setWasOffline(true);
      if (typeof global !== "undefined" && global.toast) {
        global.toast.warning(
          "You are offline. Some features may not be available.",
          {
            label: "Retry",
            onPress: () => {
              // Trigger network recheck
              const event = new Event("network-retry");
              window.dispatchEvent?.(event);
            },
          }
        );
      }
    } else if (networkStatus.isConnected === true && wasOffline) {
      setWasOffline(false);
      if (typeof global !== "undefined" && global.toast) {
        global.toast.success("You are back online!");
      }
    }
  }, [networkStatus.isConnected, wasOffline]);

  return {
    isOffline: networkStatus.isConnected === false,
    networkStatus,
  };
};
