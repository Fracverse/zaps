import React, { useState } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  Platform,
  useWindowDimensions,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter, Stack } from "expo-router";
import { COLORS } from "../src/constants/colors";

export default function ScanScreen() {
  const router = useRouter();
  const { width } = useWindowDimensions();
  const [flashOn, setFlashOn] = useState(false);

  const isTablet = width >= 600;
  const maxScannerWidth = isTablet ? 400 : 320;
  const scannerSize = Math.min(width - 40, maxScannerWidth);
  const horizontalPadding = Math.max(20, (width - scannerSize) / 2);

  return (
    <SafeAreaView style={styles.container} edges={["top"]}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
          accessibilityLabel="Go back"
          accessibilityRole="button"
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Scan qr code</Text>
        <View style={styles.headerSpacer} />
      </View>

      <View
        style={[
          styles.scannerWrapper,
          { paddingHorizontal: horizontalPadding },
        ]}
      >
        <View
          style={[
            styles.scannerViewport,
            {
              width: scannerSize,
              height: scannerSize,
            },
          ]}
        >
          {/* Placeholder for camera feed - dashed border per design */}
          <View style={styles.scannerPlaceholder} />
        </View>
      </View>

      <View style={styles.actions}>
        <TouchableOpacity
          style={styles.pillButton}
          onPress={() => setFlashOn(!flashOn)}
          activeOpacity={0.8}
          accessibilityLabel={flashOn ? "Turn off flash" : "Turn on flash"}
          accessibilityRole="button"
        >
          <Ionicons
            name={flashOn ? "flash" : "flash-outline"}
            size={20}
            color={COLORS.primary}
            style={styles.pillButtonIcon}
          />
          <Text style={styles.pillButtonText}>
            {flashOn ? "Turn off flash" : "Turn on flash"}
          </Text>
        </TouchableOpacity>
        <TouchableOpacity
          style={styles.pillButton}
          onPress={() => {}}
          activeOpacity={0.8}
          accessibilityLabel="Select photo from gallery"
          accessibilityRole="button"
        >
          <Ionicons
            name="image-outline"
            size={20}
            color={COLORS.primary}
            style={styles.pillButtonIcon}
          />
          <Text style={styles.pillButtonText}>Select photo</Text>
        </TouchableOpacity>
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.white,
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 15,
  },
  backButton: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: "center",
    alignItems: "center",
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  headerSpacer: {
    width: 40,
  },
  scannerWrapper: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
  },
  scannerViewport: {
    borderRadius: 16,
    overflow: "hidden",
    borderWidth: 2,
    borderStyle: "dashed",
    borderColor: "#CCCCCC",
  },
  scannerPlaceholder: {
    flex: 1,
    backgroundColor: "#F5F5F5",
  },
  actions: {
    flexDirection: "row",
    justifyContent: "center",
    alignItems: "center",
    gap: 12,
    paddingHorizontal: 20,
    paddingBottom: Platform.OS === "ios" ? 34 : 24,
    flexWrap: "wrap",
  },
  pillButton: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    paddingVertical: 14,
    paddingHorizontal: 20,
    borderRadius: 100,
    backgroundColor: COLORS.white,
    borderWidth: 1.5,
    borderColor: COLORS.primary,
  },
  pillButtonIcon: {
    marginRight: 8,
  },
  pillButtonText: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
});
