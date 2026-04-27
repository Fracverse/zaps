import React, { useEffect, useState, useCallback } from "react";
import {
  View,
  Text,
  StyleSheet,
  Animated,
  TouchableOpacity,
} from "react-native";
import { Ionicons } from "@expo/vector-icons";

export type ToastType = "success" | "error" | "warning" | "info";

interface ToastProps {
  message: string;
  type?: ToastType;
  duration?: number;
  onHide?: () => void;
  action?: {
    label: string;
    onPress: () => void;
  };
}

export const Toast: React.FC<ToastProps> = ({
  message,
  type = "info",
  duration = 3000,
  onHide,
  action,
}) => {
  const [fadeAnim] = useState(new Animated.Value(0));
  const [slideAnim] = useState(new Animated.Value(-100));

  const hideToast = useCallback(() => {
    Animated.parallel([
      Animated.timing(fadeAnim, {
        toValue: 0,
        duration: 300,
        useNativeDriver: true,
      }),
      Animated.timing(slideAnim, {
        toValue: -100,
        duration: 300,
        useNativeDriver: true,
      }),
    ]).start(() => {
      onHide?.();
    });
  }, [fadeAnim, slideAnim, onHide]);

  useEffect(() => {
    // Show toast
    Animated.parallel([
      Animated.timing(fadeAnim, {
        toValue: 1,
        duration: 300,
        useNativeDriver: true,
      }),
      Animated.timing(slideAnim, {
        toValue: 0,
        duration: 300,
        useNativeDriver: true,
      }),
    ]).start();

    // Auto hide after duration
    const timer = setTimeout(() => {
      hideToast();
    }, duration);

    return () => clearTimeout(timer);
  }, [duration, fadeAnim, slideAnim, hideToast]);

  const getIcon = () => {
    switch (type) {
      case "success":
        return "checkmark-circle";
      case "error":
        return "close-circle";
      case "warning":
        return "warning";
      case "info":
        return "information-circle";
      default:
        return "information-circle";
    }
  };

  const getIconColor = () => {
    switch (type) {
      case "success":
        return "#4CAF50";
      case "error":
        return "#F44336";
      case "warning":
        return "#FF9800";
      case "info":
        return "#2196F3";
      default:
        return "#2196F3";
    }
  };

  const getBackgroundColor = () => {
    switch (type) {
      case "success":
        return "#E8F5E8";
      case "error":
        return "#FFEBEE";
      case "warning":
        return "#FFF3E0";
      case "info":
        return "#E3F2FD";
      default:
        return "#E3F2FD";
    }
  };

  return (
    <Animated.View
      style={[
        styles.container,
        {
          backgroundColor: getBackgroundColor(),
          opacity: fadeAnim,
          transform: [{ translateY: slideAnim }],
        },
      ]}
    >
      <View style={styles.content}>
        <Ionicons
          name={getIcon()}
          size={20}
          color={getIconColor()}
          style={styles.icon}
        />
        <Text
          style={[styles.message, { color: getIconColor() }]}
          numberOfLines={2}
        >
          {message}
        </Text>
        {action && (
          <TouchableOpacity
            onPress={action.onPress}
            style={styles.actionButton}
          >
            <Text style={[styles.actionText, { color: getIconColor() }]}>
              {action.label}
            </Text>
          </TouchableOpacity>
        )}
        <TouchableOpacity onPress={hideToast} style={styles.closeButton}>
          <Ionicons name="close" size={16} color={getIconColor()} />
        </TouchableOpacity>
      </View>
    </Animated.View>
  );
};

// Toast Manager Component
export const ToastManager: React.FC = () => {
  const [toasts, setToasts] = useState<{ id: string; props: ToastProps }[]>([]);

  const showToast = (props: Omit<ToastProps, "onHide">) => {
    const id = Date.now().toString();
    const newToast = { id, props };

    setToasts((prev) => [...prev, newToast]);
  };

  const hideToast = (id: string) => {
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
  };

  // Global toast function
  useEffect(() => {
    const toastFunctions = {
      show: showToast,
      success: (message: string, action?: ToastProps["action"]) =>
        showToast({ message, type: "success", action }),
      error: (message: string, action?: ToastProps["action"]) =>
        showToast({ message, type: "error", action }),
      warning: (message: string, action?: ToastProps["action"]) =>
        showToast({ message, type: "warning", action }),
      info: (message: string, action?: ToastProps["action"]) =>
        showToast({ message, type: "info", action }),
    };

    (global as any).toast = toastFunctions;
  }, []);

  return (
    <View style={styles.toastContainer}>
      {toasts.map(({ id, props }) => (
        <Toast key={id} {...props} onHide={() => hideToast(id)} />
      ))}
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    marginHorizontal: 16,
    marginVertical: 8,
    padding: 12,
    borderRadius: 8,
    shadowColor: "#000",
    shadowOffset: {
      width: 0,
      height: 2,
    },
    shadowOpacity: 0.1,
    shadowRadius: 3.84,
    elevation: 5,
  },
  content: {
    flexDirection: "row",
    alignItems: "center",
    flex: 1,
  },
  icon: {
    marginRight: 8,
  },
  message: {
    flex: 1,
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    lineHeight: 20,
  },
  actionButton: {
    marginLeft: 8,
    paddingHorizontal: 8,
    paddingVertical: 4,
  },
  actionText: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    textDecorationLine: "underline",
  },
  closeButton: {
    marginLeft: 8,
    padding: 4,
  },
  toastContainer: {
    position: "absolute",
    top: 50,
    left: 0,
    right: 0,
    zIndex: 9999,
  },
});

// Global toast types
declare global {
  let toast: {
    show: (props: Omit<ToastProps, "onHide">) => void;
    success: (message: string, action?: ToastProps["action"]) => void;
    error: (message: string, action?: ToastProps["action"]) => void;
    warning: (message: string, action?: ToastProps["action"]) => void;
    info: (message: string, action?: ToastProps["action"]) => void;
  };
}
