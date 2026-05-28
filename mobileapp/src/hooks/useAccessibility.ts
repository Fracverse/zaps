import { useReducedMotion, useAnimatedStyle } from "react-native-reanimated";
import { AccessibilityInfo, useColorScheme } from "react-native";
import { useEffect, useState } from "react";

export function useAccessibility() {
  const prefersReducedMotion = useReducedMotion();
  const colorScheme = useColorScheme();
  const [isScreenReaderEnabled, setIsScreenReaderEnabled] = useState(false);
  const [isBoldTextEnabled, setIsBoldTextEnabled] = useState(false);

  useEffect(() => {
    AccessibilityInfo.isScreenReaderEnabled().then(setIsScreenReaderEnabled);
    AccessibilityInfo.isBoldTextEnabled().then(setIsBoldTextEnabled);

    const readerSub = AccessibilityInfo.addEventListener(
      "screenReaderChanged",
      setIsScreenReaderEnabled
    );
    const boldSub = AccessibilityInfo.addEventListener(
      "boldTextChanged",
      setIsBoldTextEnabled
    );

    return () => {
      readerSub.remove();
      boldSub.remove();
    };
  }, []);

  /**
   * Returns animation config that respects reduced-motion preference.
   * Pass `duration` for normal animation; returns 0 when reduced motion is on.
   */
  const animationDuration = (duration: number) =>
    prefersReducedMotion ? 0 : duration;

  return {
    prefersReducedMotion,
    isScreenReaderEnabled,
    isBoldTextEnabled,
    isDarkMode: colorScheme === "dark",
    animationDuration,
  };
}
