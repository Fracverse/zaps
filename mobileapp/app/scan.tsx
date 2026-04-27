import React, { useState, useCallback, useEffect } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  Platform,
  useWindowDimensions,
  TextInput,
  ScrollView,
  ActivityIndicator,
  Linking,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter, Stack } from "expo-router";
import { CameraView, FlashMode } from "expo-camera";
import * as ImagePicker from "expo-image-picker";
import { COLORS } from "../src/constants/colors";
import { useCameraPermission } from "../src/hooks/useCameraPermission";
import {
  parseSep0007Uri,
  detectQrContentType,
  Sep0007PaymentParams,
} from "../src/utils/sep0007";

// ─── Types ────────────────────────────────────────────────────────────────────

type ScreenState = "scanner" | "manual" | "preview" | "error";

interface PaymentPreview {
  destination: string;
  amount?: string;
  asset?: string;
  memo?: string;
  msg?: string;
  origin_domain?: string;
  rawUri: string;
}

// ─── Sub-components ───────────────────────────────────────────────────────────

function PermissionDenied({ onOpenSettings }: { onOpenSettings: () => void }) {
  return (
    <View style={styles.permissionContainer}>
      <Ionicons name="camera-outline" size={64} color="#ccc" />
      <Text style={styles.permissionTitle}>Camera Access Required</Text>
      <Text style={styles.permissionSubtext}>
        Allow camera access to scan QR codes for payments.
      </Text>
      <TouchableOpacity style={styles.settingsButton} onPress={onOpenSettings}>
        <Text style={styles.settingsButtonText}>Open Settings</Text>
      </TouchableOpacity>
    </View>
  );
}

function PermissionRequest({
  onRequest,
  loading,
}: {
  onRequest: () => void;
  loading: boolean;
}) {
  return (
    <View style={styles.permissionContainer}>
      <Ionicons name="qr-code-outline" size={64} color={COLORS.primary} />
      <Text style={styles.permissionTitle}>Scan to Pay</Text>
      <Text style={styles.permissionSubtext}>
        We need camera access to scan QR codes.
      </Text>
      <TouchableOpacity
        style={styles.allowButton}
        onPress={onRequest}
        disabled={loading}
      >
        {loading ? (
          <ActivityIndicator color={COLORS.secondary} />
        ) : (
          <Text style={styles.allowButtonText}>Allow Camera</Text>
        )}
      </TouchableOpacity>
    </View>
  );
}

function ScannerOverlay({ size }: { size: number }) {
  const cornerSize = 28;
  const cornerThickness = 4;
  const cornerColor = COLORS.primary;

  return (
    <View style={[styles.overlay, { width: size, height: size }]}>
      {/* Top-left */}
      <View
        style={[
          styles.corner,
          {
            top: 0,
            left: 0,
            borderTopWidth: cornerThickness,
            borderLeftWidth: cornerThickness,
            borderColor: cornerColor,
            width: cornerSize,
            height: cornerSize,
            borderTopLeftRadius: 8,
          },
        ]}
      />
      {/* Top-right */}
      <View
        style={[
          styles.corner,
          {
            top: 0,
            right: 0,
            borderTopWidth: cornerThickness,
            borderRightWidth: cornerThickness,
            borderColor: cornerColor,
            width: cornerSize,
            height: cornerSize,
            borderTopRightRadius: 8,
          },
        ]}
      />
      {/* Bottom-left */}
      <View
        style={[
          styles.corner,
          {
            bottom: 0,
            left: 0,
            borderBottomWidth: cornerThickness,
            borderLeftWidth: cornerThickness,
            borderColor: cornerColor,
            width: cornerSize,
            height: cornerSize,
            borderBottomLeftRadius: 8,
          },
        ]}
      />
      {/* Bottom-right */}
      <View
        style={[
          styles.corner,
          {
            bottom: 0,
            right: 0,
            borderBottomWidth: cornerThickness,
            borderRightWidth: cornerThickness,
            borderColor: cornerColor,
            width: cornerSize,
            height: cornerSize,
            borderBottomRightRadius: 8,
          },
        ]}
      />
    </View>
  );
}

function PaymentPreviewCard({
  preview,
  onConfirm,
  onCancel,
}: {
  preview: PaymentPreview;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const truncate = (s: string, n = 20) =>
    s.length > n ? `${s.slice(0, n)}…` : s;

  return (
    <ScrollView
      style={styles.previewScroll}
      contentContainerStyle={styles.previewContent}
      showsVerticalScrollIndicator={false}
    >
      <View style={styles.previewCard}>
        <View style={styles.previewIconRow}>
          <View style={styles.previewIconCircle}>
            <Ionicons name="qr-code" size={32} color={COLORS.primary} />
          </View>
        </View>

        <Text style={styles.previewTitle}>Payment Details</Text>

        <View style={styles.previewDivider} />

        <PreviewRow
          label="Recipient"
          value={truncate(preview.destination, 24)}
          mono
        />
        {preview.amount && (
          <PreviewRow
            label="Amount"
            value={`${preview.amount}${preview.asset ? ` ${preview.asset}` : ""}`}
          />
        )}
        {preview.memo && <PreviewRow label="Memo" value={preview.memo} />}
        {preview.msg && <PreviewRow label="Message" value={preview.msg} />}
        {preview.origin_domain && (
          <PreviewRow label="Origin" value={preview.origin_domain} />
        )}
      </View>

      <View style={styles.previewActions}>
        <TouchableOpacity
          style={styles.cancelBtn}
          onPress={onCancel}
          activeOpacity={0.8}
        >
          <Text style={styles.cancelBtnText}>Cancel</Text>
        </TouchableOpacity>
        <TouchableOpacity
          style={styles.confirmBtn}
          onPress={onConfirm}
          activeOpacity={0.8}
        >
          <Ionicons
            name="checkmark"
            size={18}
            color={COLORS.secondary}
            style={{ marginRight: 6 }}
          />
          <Text style={styles.confirmBtnText}>Confirm</Text>
        </TouchableOpacity>
      </View>
    </ScrollView>
  );
}

function PreviewRow({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <View style={styles.previewRow}>
      <Text style={styles.previewLabel}>{label}</Text>
      <Text style={[styles.previewValue, mono && styles.previewValueMono]}>
        {value}
      </Text>
    </View>
  );
}

// ─── Main Screen ──────────────────────────────────────────────────────────────

export default function ScanScreen() {
  const router = useRouter();
  const { width } = useWindowDimensions();
  const { granted, denied, undetermined, loading, requestPermission } =
    useCameraPermission();

  const [screenState, setScreenState] = useState<ScreenState>("scanner");
  const [flashMode, setFlashMode] = useState<FlashMode>("off");
  const [scanned, setScanned] = useState(false);
  const [manualAddress, setManualAddress] = useState("");
  const [preview, setPreview] = useState<PaymentPreview | null>(null);
  const [errorMsg, setErrorMsg] = useState("");

  const isTablet = width >= 600;
  const scannerSize = Math.min(width - 48, isTablet ? 380 : 300);

  // Auto-request on mount if undetermined
  useEffect(() => {
    if (undetermined) {
      requestPermission();
    }
  }, [undetermined, requestPermission]);

  const handleQrData = useCallback(
    (data: string) => {
      if (scanned) return;
      setScanned(true);

      const contentType = detectQrContentType(data);

      if (contentType === "sep0007") {
        const result = parseSep0007Uri(data);
        if (!result.valid) {
          setErrorMsg(result.error);
          setScreenState("error");
          return;
        }
        if (result.operation === "pay") {
          const p = result.params as Sep0007PaymentParams;
          setPreview({
            destination: p.destination,
            amount: p.amount,
            asset: p.asset_code,
            memo: p.memo,
            msg: p.msg,
            origin_domain: p.origin_domain,
            rawUri: data,
          });
          setScreenState("preview");
        } else {
          // tx operation — show raw info
          setPreview({
            destination: "XDR Transaction",
            msg: "This QR contains a signed transaction envelope.",
            rawUri: data,
          });
          setScreenState("preview");
        }
      } else if (contentType === "stellar_address") {
        setPreview({
          destination: data.trim(),
          rawUri: data.trim(),
        });
        setScreenState("preview");
      } else {
        setErrorMsg(
          `Unrecognized QR code content.\n\nExpected a SEP-0007 payment URI (web+stellar:pay?…) or a Stellar address.`
        );
        setScreenState("error");
      }
    },
    [scanned]
  );

  const handleBarcodeScanned = useCallback(
    ({ data }: { data: string }) => {
      handleQrData(data);
    },
    [handleQrData]
  );

  const handleGalleryPick = useCallback(async () => {
    const result = await ImagePicker.launchImageLibraryAsync({
      mediaTypes: ["images"],
      allowsEditing: false,
      quality: 1,
    });

    if (result.canceled || !result.assets?.[0]) return;

    // expo-camera doesn't expose a static QR decoder for images.
    // Show a toast guiding the user to use the camera or manual entry.
    if (typeof global !== "undefined" && global.toast) {
      global.toast.info(
        "For best results, scan the QR code directly with your camera or use manual entry."
      );
    }
  }, []);

  const handleManualSubmit = useCallback(() => {
    const trimmed = manualAddress.trim();
    if (!trimmed) return;
    handleQrData(trimmed);
  }, [manualAddress, handleQrData]);

  const handleConfirmPayment = useCallback(() => {
    if (!preview) return;
    // Navigate to transfer screen pre-filled with scanned data
    const params = new URLSearchParams();
    params.set("destination", preview.destination);
    if (preview.amount) params.set("amount", preview.amount);
    if (preview.asset) params.set("asset", preview.asset);
    if (preview.memo) params.set("memo", preview.memo);
    router.push(`/transfer?${params.toString()}` as any);
  }, [preview, router]);

  const resetScanner = useCallback(() => {
    setScanned(false);
    setPreview(null);
    setErrorMsg("");
    setScreenState("scanner");
    setManualAddress("");
  }, []);

  const openSettings = useCallback(() => {
    Linking.openSettings();
  }, []);

  // ── Render helpers ──────────────────────────────────────────────────────────

  const renderHeader = (title: string) => (
    <View style={styles.header}>
      <TouchableOpacity
        onPress={() =>
          screenState !== "scanner" ? resetScanner() : router.back()
        }
        style={styles.backButton}
        accessibilityLabel="Go back"
        accessibilityRole="button"
      >
        <Ionicons name="arrow-back" size={24} color={COLORS.black} />
      </TouchableOpacity>
      <Text style={styles.headerTitle}>{title}</Text>
      <View style={styles.headerSpacer} />
    </View>
  );

  // ── Permission screens ──────────────────────────────────────────────────────

  if (denied) {
    return (
      <SafeAreaView style={styles.container} edges={["top"]}>
        <Stack.Screen options={{ headerShown: false }} />
        {renderHeader("Scan QR Code")}
        <PermissionDenied onOpenSettings={openSettings} />
      </SafeAreaView>
    );
  }

  if (undetermined || (!granted && loading)) {
    return (
      <SafeAreaView style={styles.container} edges={["top"]}>
        <Stack.Screen options={{ headerShown: false }} />
        {renderHeader("Scan QR Code")}
        <PermissionRequest onRequest={requestPermission} loading={loading} />
      </SafeAreaView>
    );
  }

  // ── Preview screen ──────────────────────────────────────────────────────────

  if (screenState === "preview" && preview) {
    return (
      <SafeAreaView style={styles.container} edges={["top"]}>
        <Stack.Screen options={{ headerShown: false }} />
        {renderHeader("Payment Details")}
        <PaymentPreviewCard
          preview={preview}
          onConfirm={handleConfirmPayment}
          onCancel={resetScanner}
        />
      </SafeAreaView>
    );
  }

  // ── Error screen ────────────────────────────────────────────────────────────

  if (screenState === "error") {
    return (
      <SafeAreaView style={styles.container} edges={["top"]}>
        <Stack.Screen options={{ headerShown: false }} />
        {renderHeader("Scan QR Code")}
        <View style={styles.errorContainer}>
          <Ionicons name="close-circle-outline" size={64} color="#EF4444" />
          <Text style={styles.errorTitle}>Invalid QR Code</Text>
          <Text style={styles.errorMsg}>{errorMsg}</Text>
          <TouchableOpacity style={styles.retryBtn} onPress={resetScanner}>
            <Text style={styles.retryBtnText}>Try Again</Text>
          </TouchableOpacity>
          <TouchableOpacity
            style={styles.manualFallbackBtn}
            onPress={() => {
              setScreenState("manual");
              setScanned(false);
              setErrorMsg("");
            }}
          >
            <Text style={styles.manualFallbackText}>
              Enter Address Manually
            </Text>
          </TouchableOpacity>
        </View>
      </SafeAreaView>
    );
  }

  // ── Manual entry screen ─────────────────────────────────────────────────────

  if (screenState === "manual") {
    return (
      <SafeAreaView style={styles.container} edges={["top"]}>
        <Stack.Screen options={{ headerShown: false }} />
        {renderHeader("Enter Address")}
        <View style={styles.manualContainer}>
          <Text style={styles.manualLabel}>
            Enter a Stellar address or SEP-0007 URI
          </Text>
          <TextInput
            style={styles.manualInput}
            placeholder="G… or web+stellar:pay?…"
            placeholderTextColor="#999"
            value={manualAddress}
            onChangeText={setManualAddress}
            autoCapitalize="none"
            autoCorrect={false}
            multiline
            numberOfLines={3}
          />
          <TouchableOpacity
            style={[
              styles.manualSubmitBtn,
              !manualAddress.trim() && styles.manualSubmitBtnDisabled,
            ]}
            onPress={handleManualSubmit}
            disabled={!manualAddress.trim()}
          >
            <Text style={styles.manualSubmitText}>Continue</Text>
          </TouchableOpacity>
          <TouchableOpacity style={styles.backToScanBtn} onPress={resetScanner}>
            <Ionicons
              name="camera-outline"
              size={18}
              color={COLORS.primary}
              style={{ marginRight: 6 }}
            />
            <Text style={styles.backToScanText}>Back to Scanner</Text>
          </TouchableOpacity>
        </View>
      </SafeAreaView>
    );
  }

  // ── Camera scanner ──────────────────────────────────────────────────────────

  return (
    <SafeAreaView style={styles.container} edges={["top"]}>
      <Stack.Screen options={{ headerShown: false }} />
      {renderHeader("Scan QR Code")}

      <View style={styles.scannerWrapper}>
        <View
          style={[
            styles.cameraContainer,
            { width: scannerSize, height: scannerSize },
          ]}
        >
          {granted && (
            <CameraView
              style={StyleSheet.absoluteFill}
              facing="back"
              flash={flashMode}
              barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
              onBarcodeScanned={scanned ? undefined : handleBarcodeScanned}
            />
          )}
          <ScannerOverlay size={scannerSize} />
        </View>

        <Text style={styles.scanHint}>
          Point your camera at a QR code to pay
        </Text>
      </View>

      <View style={styles.actions}>
        <TouchableOpacity
          style={styles.pillButton}
          onPress={() =>
            setFlashMode((prev) => (prev === "off" ? "on" : "off"))
          }
          activeOpacity={0.8}
          accessibilityLabel={
            flashMode === "on" ? "Turn off flash" : "Turn on flash"
          }
          accessibilityRole="button"
        >
          <Ionicons
            name={flashMode === "on" ? "flash" : "flash-outline"}
            size={20}
            color={COLORS.primary}
            style={styles.pillButtonIcon}
          />
          <Text style={styles.pillButtonText}>
            {flashMode === "on" ? "Flash off" : "Flash on"}
          </Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.pillButton}
          onPress={handleGalleryPick}
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
          <Text style={styles.pillButtonText}>Gallery</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.pillButton}
          onPress={() => setScreenState("manual")}
          activeOpacity={0.8}
          accessibilityLabel="Enter address manually"
          accessibilityRole="button"
        >
          <Ionicons
            name="create-outline"
            size={20}
            color={COLORS.primary}
            style={styles.pillButtonIcon}
          />
          <Text style={styles.pillButtonText}>Manual</Text>
        </TouchableOpacity>
      </View>
    </SafeAreaView>
  );
}

// ─── Styles ───────────────────────────────────────────────────────────────────

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
  // Scanner
  scannerWrapper: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    paddingHorizontal: 24,
  },
  cameraContainer: {
    borderRadius: 16,
    overflow: "hidden",
    backgroundColor: "#000",
  },
  overlay: {
    position: "absolute",
    top: 0,
    left: 0,
  },
  corner: {
    position: "absolute",
  },
  scanHint: {
    marginTop: 20,
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    textAlign: "center",
  },
  // Actions
  actions: {
    flexDirection: "row",
    justifyContent: "center",
    alignItems: "center",
    gap: 10,
    paddingHorizontal: 20,
    paddingBottom: Platform.OS === "ios" ? 34 : 24,
    flexWrap: "wrap",
  },
  pillButton: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    paddingVertical: 12,
    paddingHorizontal: 16,
    borderRadius: 100,
    backgroundColor: COLORS.white,
    borderWidth: 1.5,
    borderColor: COLORS.primary,
  },
  pillButtonIcon: {
    marginRight: 6,
  },
  pillButtonText: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  // Permission
  permissionContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    paddingHorizontal: 32,
    gap: 16,
  },
  permissionTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    textAlign: "center",
  },
  permissionSubtext: {
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    textAlign: "center",
    lineHeight: 22,
  },
  allowButton: {
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    paddingVertical: 14,
    paddingHorizontal: 32,
    marginTop: 8,
  },
  allowButtonText: {
    color: COLORS.secondary,
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
  },
  settingsButton: {
    borderWidth: 1.5,
    borderColor: COLORS.primary,
    borderRadius: 100,
    paddingVertical: 14,
    paddingHorizontal: 32,
    marginTop: 8,
  },
  settingsButtonText: {
    color: COLORS.primary,
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
  },
  // Preview
  previewScroll: {
    flex: 1,
  },
  previewContent: {
    paddingHorizontal: 20,
    paddingTop: 8,
    paddingBottom: 32,
  },
  previewCard: {
    backgroundColor: COLORS.white,
    borderRadius: 24,
    padding: 24,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    elevation: 2,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.05,
    shadowRadius: 8,
    marginBottom: 24,
  },
  previewIconRow: {
    alignItems: "center",
    marginBottom: 16,
  },
  previewIconCircle: {
    width: 64,
    height: 64,
    borderRadius: 32,
    backgroundColor: "#F0FDF4",
    justifyContent: "center",
    alignItems: "center",
  },
  previewTitle: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    textAlign: "center",
    marginBottom: 16,
  },
  previewDivider: {
    height: 1,
    backgroundColor: "#F0F0F0",
    marginBottom: 16,
  },
  previewRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "flex-start",
    paddingVertical: 10,
    borderBottomWidth: 1,
    borderBottomColor: "#F8F8F8",
  },
  previewLabel: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    flex: 1,
  },
  previewValue: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    flex: 2,
    textAlign: "right",
  },
  previewValueMono: {
    fontFamily: "Outfit_400Regular",
    fontSize: 12,
  },
  previewActions: {
    flexDirection: "row",
    gap: 12,
  },
  cancelBtn: {
    flex: 1,
    borderWidth: 1.5,
    borderColor: COLORS.primary,
    borderRadius: 100,
    paddingVertical: 16,
    alignItems: "center",
  },
  cancelBtnText: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  confirmBtn: {
    flex: 2,
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    paddingVertical: 16,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
  },
  confirmBtnText: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.secondary,
  },
  // Error
  errorContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    paddingHorizontal: 32,
    gap: 16,
  },
  errorTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  errorMsg: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    textAlign: "center",
    lineHeight: 22,
  },
  retryBtn: {
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    paddingVertical: 14,
    paddingHorizontal: 32,
    marginTop: 8,
  },
  retryBtnText: {
    color: COLORS.secondary,
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
  },
  manualFallbackBtn: {
    paddingVertical: 12,
  },
  manualFallbackText: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
    color: COLORS.primary,
    textDecorationLine: "underline",
  },
  // Manual entry
  manualContainer: {
    flex: 1,
    paddingHorizontal: 20,
    paddingTop: 16,
  },
  manualLabel: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
    color: "#666",
    marginBottom: 12,
  },
  manualInput: {
    borderWidth: 1.5,
    borderColor: "#E0E0E0",
    borderRadius: 16,
    paddingHorizontal: 16,
    paddingVertical: 14,
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
    minHeight: 90,
    textAlignVertical: "top",
    marginBottom: 16,
  },
  manualSubmitBtn: {
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    paddingVertical: 16,
    alignItems: "center",
    marginBottom: 12,
  },
  manualSubmitBtnDisabled: {
    opacity: 0.5,
  },
  manualSubmitText: {
    color: COLORS.secondary,
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
  },
  backToScanBtn: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    paddingVertical: 12,
  },
  backToScanText: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
    color: COLORS.primary,
  },
});
