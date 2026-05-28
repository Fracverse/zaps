/**
 * MnemonicBackup screen
 *
 * - Blocks screenshots natively via expo-screen-capture
 * - Displays the 12/24-word phrase ONCE
 * - Requires explicit user confirmation before proceeding
 * - Clipboard auto-clears after 30 s via walletSecurity.copyWithAutoClear
 */
import React, { useEffect, useState, useCallback, useRef } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  Alert,
  ActivityIndicator,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import { Ionicons } from "@expo/vector-icons";
import * as ScreenCapture from "expo-screen-capture";

import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import {
  createNewWallet,
  copyWithAutoClear,
  clearClipboard,
  type GeneratedWallet,
} from "../src/services/walletSecurity";

// ── Word grid ─────────────────────────────────────────────────────────────────

interface WordTileProps {
  index: number;
  word: string;
}

const WordTile = ({ index, word }: WordTileProps) => (
  <View style={styles.wordTile}>
    <Text style={styles.wordIndex}>{index + 1}</Text>
    <Text style={styles.wordText}>{word}</Text>
  </View>
);

// ── Screen ────────────────────────────────────────────────────────────────────

export default function MnemonicBackupScreen() {
  const router = useRouter();

  const [wallet, setWallet] = useState<GeneratedWallet | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [copied, setCopied] = useState(false);
  const [copyCountdown, setCopyCountdown] = useState<number | null>(null);

  // Keep a ref to the clipboard cancel fn so we can clean up on unmount
  const cancelClipboardRef = useRef<(() => void) | null>(null);
  const countdownRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // ── Screenshot prevention ─────────────────────────────────────────────────

  useEffect(() => {
    let subscription: ScreenCapture.Subscription | null = null;

    const setup = async () => {
      // Prevent screenshots while this screen is mounted
      await ScreenCapture.preventScreenCaptureAsync();

      // Also listen for any capture attempts and warn the user
      subscription = ScreenCapture.addScreenshotListener(() => {
        Alert.alert(
          "Screenshot Detected",
          "Screenshots of your recovery phrase are a security risk. Please write it down on paper instead.",
          [{ text: "OK" }]
        );
      });
    };

    setup();

    return () => {
      // Re-allow screenshots when leaving this screen
      ScreenCapture.allowScreenCaptureAsync();
      subscription?.remove();
    };
  }, []);

  // ── Wallet generation ─────────────────────────────────────────────────────

  useEffect(() => {
    const generate = async () => {
      try {
        const generated = await createNewWallet(24);
        setWallet(generated);
      } catch (err) {
        setError(
          err instanceof Error ? err.message : "Failed to generate wallet"
        );
      } finally {
        setLoading(false);
      }
    };
    generate();
  }, []);

  // ── Cleanup on unmount ────────────────────────────────────────────────────

  useEffect(() => {
    return () => {
      // Clear clipboard and any timers when leaving the screen
      cancelClipboardRef.current?.();
      if (countdownRef.current) clearInterval(countdownRef.current);
    };
  }, []);

  // ── Copy handler ──────────────────────────────────────────────────────────

  const handleCopy = useCallback(() => {
    if (!wallet) return;

    // Cancel any previous copy timer
    cancelClipboardRef.current?.();
    if (countdownRef.current) clearInterval(countdownRef.current);

    const phrase = wallet.mnemonic;
    const cancel = copyWithAutoClear(phrase);
    cancelClipboardRef.current = cancel;

    setCopied(true);
    setCopyCountdown(30);

    // Countdown display
    countdownRef.current = setInterval(() => {
      setCopyCountdown((prev) => {
        if (prev === null || prev <= 1) {
          clearInterval(countdownRef.current!);
          countdownRef.current = null;
          setCopied(false);
          setCopyCountdown(null);
          return null;
        }
        return prev - 1;
      });
    }, 1000);
  }, [wallet]);

  // ── Continue handler ──────────────────────────────────────────────────────

  const handleContinue = useCallback(async () => {
    if (!confirmed) return;

    // Clear clipboard before navigating away
    await clearClipboard();
    if (countdownRef.current) clearInterval(countdownRef.current);
    cancelClipboardRef.current = null;

    // Navigate to account-type selection (wallet is already stored)
    router.replace("/account-type");
  }, [confirmed, router]);

  // ── Render ────────────────────────────────────────────────────────────────

  if (loading) {
    return (
      <SafeAreaView style={styles.centered}>
        <Stack.Screen options={{ headerShown: false }} />
        <ActivityIndicator size="large" color={COLORS.primary} />
        <Text style={styles.loadingText}>Generating your wallet…</Text>
      </SafeAreaView>
    );
  }

  if (error || !wallet) {
    return (
      <SafeAreaView style={styles.centered}>
        <Stack.Screen options={{ headerShown: false }} />
        <Ionicons name="alert-circle-outline" size={48} color="#E53E3E" />
        <Text style={styles.errorText}>{error ?? "Unknown error"}</Text>
        <Button
          title="Go Back"
          onPress={() => router.back()}
          variant="outline"
          style={{ marginTop: 24, width: 200 }}
        />
      </SafeAreaView>
    );
  }

  const words = wallet.mnemonic.split(" ");

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      {/* Header */}
      <View style={styles.header}>
        <TouchableOpacity
          style={styles.backButton}
          onPress={() => router.back()}
          accessibilityLabel="Go back"
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Recovery Phrase</Text>
        <View style={{ width: 40 }} />
      </View>

      <ScrollView
        style={styles.scroll}
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        {/* Warning banner */}
        <View style={styles.warningBanner}>
          <Ionicons name="warning-outline" size={20} color="#B7791F" />
          <Text style={styles.warningText}>
            Write these words down on paper. Never share them with anyone.
            Screenshots are blocked for your security.
          </Text>
        </View>

        {/* Word grid */}
        <View style={styles.wordGrid}>
          {words.map((word, i) => (
            <WordTile key={i} index={i} word={word} />
          ))}
        </View>

        {/* Copy button */}
        <TouchableOpacity
          style={[styles.copyButton, copied && styles.copyButtonActive]}
          onPress={handleCopy}
          activeOpacity={0.8}
          accessibilityLabel="Copy recovery phrase to clipboard"
        >
          <Ionicons
            name={copied ? "checkmark-circle-outline" : "copy-outline"}
            size={20}
            color={copied ? COLORS.primary : COLORS.black}
            style={{ marginRight: 8 }}
          />
          <Text
            style={[styles.copyButtonText, copied && styles.copyButtonTextActive]}
          >
            {copied
              ? `Copied — clears in ${copyCountdown}s`
              : "Copy to Clipboard"}
          </Text>
        </TouchableOpacity>

        {/* Confirmation checkbox */}
        <TouchableOpacity
          style={styles.checkboxRow}
          onPress={() => setConfirmed((v) => !v)}
          activeOpacity={0.8}
          accessibilityRole="checkbox"
          accessibilityState={{ checked: confirmed }}
        >
          <View style={[styles.checkbox, confirmed && styles.checkboxChecked]}>
            {confirmed && (
              <Ionicons name="checkmark" size={14} color={COLORS.primary} />
            )}
          </View>
          <Text style={styles.checkboxText}>
            I have written down my recovery phrase and stored it safely. I
            understand that losing it means losing access to my wallet forever.
          </Text>
        </TouchableOpacity>

        {/* Account info */}
        <View style={styles.accountInfo}>
          <Ionicons
            name="wallet-outline"
            size={16}
            color={COLORS.primary}
            style={{ marginRight: 6 }}
          />
          <Text style={styles.accountInfoText}>
            Public key:{" "}
            <Text style={styles.publicKey}>
              {wallet.accounts[0].publicKey.slice(0, 8)}…
              {wallet.accounts[0].publicKey.slice(-8)}
            </Text>
          </Text>
        </View>
      </ScrollView>

      {/* Footer CTA */}
      <View style={styles.footer}>
        <Button
          title="I've saved my phrase"
          onPress={handleContinue}
          variant="primary"
          disabled={!confirmed}
          style={!confirmed ? styles.disabledButton : undefined}
        />
      </View>
    </SafeAreaView>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.white,
  },
  centered: {
    flex: 1,
    backgroundColor: COLORS.white,
    justifyContent: "center",
    alignItems: "center",
    padding: 24,
  },
  loadingText: {
    marginTop: 16,
    fontSize: 16,
    fontFamily: "Outfit_400Regular",
    color: "#666",
  },
  errorText: {
    marginTop: 16,
    fontSize: 16,
    fontFamily: "Outfit_400Regular",
    color: "#E53E3E",
    textAlign: "center",
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 10,
  },
  backButton: {
    padding: 8,
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  scroll: {
    flex: 1,
  },
  scrollContent: {
    paddingHorizontal: 20,
    paddingBottom: 24,
  },
  warningBanner: {
    flexDirection: "row",
    backgroundColor: "#FFFBEB",
    borderRadius: 12,
    padding: 14,
    marginBottom: 20,
    borderWidth: 1,
    borderColor: "#F6E05E",
    alignItems: "flex-start",
    gap: 10,
  },
  warningText: {
    flex: 1,
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#744210",
    lineHeight: 19,
  },
  wordGrid: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 8,
    marginBottom: 20,
  },
  wordTile: {
    width: "30%",
    backgroundColor: "#F7F7F7",
    borderRadius: 10,
    paddingVertical: 10,
    paddingHorizontal: 8,
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
    borderWidth: 1,
    borderColor: "#EFEFEF",
  },
  wordIndex: {
    fontSize: 11,
    fontFamily: "Outfit_500Medium",
    color: "#999",
    minWidth: 18,
  },
  wordText: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    flexShrink: 1,
  },
  copyButton: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#F5F5F5",
    borderRadius: 100,
    height: 52,
    marginBottom: 20,
    borderWidth: 1,
    borderColor: "#E8E8E8",
  },
  copyButtonActive: {
    backgroundColor: "#F0FFF4",
    borderColor: COLORS.secondary,
  },
  copyButtonText: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
  },
  copyButtonTextActive: {
    color: COLORS.primary,
  },
  checkboxRow: {
    flexDirection: "row",
    backgroundColor: "#F5F5F5",
    borderRadius: 16,
    padding: 16,
    alignItems: "flex-start",
    marginBottom: 16,
  },
  checkbox: {
    width: 24,
    height: 24,
    borderRadius: 12,
    borderWidth: 1.5,
    borderColor: "#BBBBBB",
    marginRight: 14,
    justifyContent: "center",
    alignItems: "center",
    backgroundColor: COLORS.white,
    marginTop: 1,
    flexShrink: 0,
  },
  checkboxChecked: {
    borderColor: COLORS.primary,
    backgroundColor: "#F0FFF4",
  },
  checkboxText: {
    flex: 1,
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#333",
    lineHeight: 21,
  },
  accountInfo: {
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#F0FFF4",
    borderRadius: 10,
    padding: 12,
    borderWidth: 1,
    borderColor: COLORS.secondary,
  },
  accountInfoText: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: COLORS.primary,
  },
  publicKey: {
    fontFamily: "Outfit_500Medium",
  },
  footer: {
    padding: 20,
    paddingBottom: 36,
  },
  disabledButton: {
    opacity: 0.5,
  },
});
