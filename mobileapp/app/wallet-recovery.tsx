/**
 * WalletRecovery screen
 *
 * - Accepts a 12 or 24-word BIP39 mnemonic phrase
 * - Validates against the BIP39 wordlist before attempting key derivation
 * - Stores the derived keypair securely via walletSecurity
 * - Blocks screenshots while the phrase is visible
 * - Clears clipboard on unmount
 */
import React, { useEffect, useState, useCallback, useRef } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  TextInput,
  ScrollView,
  Alert,
  KeyboardAvoidingView,
  Platform,
  ActivityIndicator,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import { Ionicons } from "@expo/vector-icons";
import * as ScreenCapture from "expo-screen-capture";

import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import {
  validateMnemonic,
  restoreWalletFromMnemonic,
  clearClipboard,
} from "../src/services/walletSecurity";

// ── Word count selector ───────────────────────────────────────────────────────

type WordCount = 12 | 24;

interface WordCountTabProps {
  count: WordCount;
  selected: boolean;
  onPress: () => void;
}

const WordCountTab = ({ count, selected, onPress }: WordCountTabProps) => (
  <TouchableOpacity
    style={[styles.tab, selected && styles.tabSelected]}
    onPress={onPress}
    activeOpacity={0.8}
    accessibilityRole="tab"
    accessibilityState={{ selected }}
  >
    <Text style={[styles.tabText, selected && styles.tabTextSelected]}>
      {count} words
    </Text>
  </TouchableOpacity>
);

// ── Individual word input ─────────────────────────────────────────────────────

interface WordInputProps {
  index: number;
  value: string;
  isInvalid: boolean;
  onChangeText: (text: string) => void;
  onSubmitEditing: () => void;
  inputRef: React.RefObject<TextInput | null>;
}

const WordInput = ({
  index,
  value,
  isInvalid,
  onChangeText,
  onSubmitEditing,
  inputRef,
}: WordInputProps) => (
  <View style={styles.wordInputWrapper}>
    <Text style={styles.wordInputIndex}>{index + 1}</Text>
    <TextInput
      ref={inputRef as React.RefObject<TextInput>}
      style={[styles.wordInput, isInvalid && styles.wordInputInvalid]}
      value={value}
      onChangeText={(t) => onChangeText(t.toLowerCase().trim())}
      onSubmitEditing={onSubmitEditing}
      autoCapitalize="none"
      autoCorrect={false}
      spellCheck={false}
      returnKeyType="next"
      placeholder={`word ${index + 1}`}
      placeholderTextColor="#BBBBBB"
      accessibilityLabel={`Recovery word ${index + 1}`}
    />
  </View>
);

// ── Screen ────────────────────────────────────────────────────────────────────

export default function WalletRecoveryScreen() {
  const router = useRouter();

  const [wordCount, setWordCount] = useState<WordCount>(24);
  const [words, setWords] = useState<string[]>(Array(24).fill(""));
  const [invalidIndices, setInvalidIndices] = useState<Set<number>>(new Set());
  const [pasteMode, setPasteMode] = useState(false);
  const [pasteText, setPasteText] = useState("");
  const [loading, setLoading] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);

  // Refs for sequential focus
  const inputRefs = useRef<React.RefObject<TextInput | null>[]>(
    Array.from({ length: 24 }, () => React.createRef<TextInput | null>())
  );

  // ── Screenshot prevention ─────────────────────────────────────────────────

  useEffect(() => {
    let subscription: ScreenCapture.Subscription | null = null;

    const setup = async () => {
      await ScreenCapture.preventScreenCaptureAsync();
      subscription = ScreenCapture.addScreenshotListener(() => {
        Alert.alert(
          "Screenshot Detected",
          "Screenshots of your recovery phrase are a security risk.",
          [{ text: "OK" }]
        );
      });
    };

    setup();

    return () => {
      ScreenCapture.allowScreenCaptureAsync();
      subscription?.remove();
    };
  }, []);

  // ── Clear clipboard on unmount ────────────────────────────────────────────

  useEffect(() => {
    return () => {
      clearClipboard();
    };
  }, []);

  // ── Word count change ─────────────────────────────────────────────────────

  const handleWordCountChange = useCallback((count: WordCount) => {
    setWordCount(count);
    setWords(Array(count).fill(""));
    setInvalidIndices(new Set());
    setValidationError(null);
    setPasteText("");
  }, []);

  // ── Individual word update ────────────────────────────────────────────────

  const handleWordChange = useCallback((index: number, text: string) => {
    setWords((prev) => {
      const next = [...prev];
      next[index] = text;
      return next;
    });
    // Clear invalid state for this word as user types
    setInvalidIndices((prev) => {
      const next = new Set(prev);
      next.delete(index);
      return next;
    });
    setValidationError(null);
  }, []);

  // ── Paste mode ────────────────────────────────────────────────────────────

  const handlePasteApply = useCallback(() => {
    const parsed = pasteText
      .trim()
      .toLowerCase()
      .split(/\s+/)
      .filter(Boolean);

    if (parsed.length !== wordCount) {
      setValidationError(
        `Expected ${wordCount} words but got ${parsed.length}. Please check your phrase.`
      );
      return;
    }

    setWords(parsed);
    setInvalidIndices(new Set());
    setValidationError(null);
    setPasteMode(false);
    // Clear the paste text field immediately for security
    setPasteText("");
  }, [pasteText, wordCount]);

  // ── Validation & recovery ─────────────────────────────────────────────────

  const handleRecover = useCallback(async () => {
    setValidationError(null);

    const activeWords = words.slice(0, wordCount);

    // Check all fields are filled
    const emptyIndices = activeWords
      .map((w, i) => (w.trim() === "" ? i : -1))
      .filter((i) => i !== -1);

    if (emptyIndices.length > 0) {
      setInvalidIndices(new Set(emptyIndices));
      setValidationError(
        `Please fill in all ${wordCount} words before continuing.`
      );
      return;
    }

    const phrase = activeWords.join(" ");

    // BIP39 validation
    if (!validateMnemonic(phrase)) {
      // Try to identify which words are invalid
      // (bip39 wordlist check per word)
      const { wordlists } = await import("bip39");
      const list: string[] = wordlists.english;
      const wordSet = new Set(list);
      const bad = new Set<number>();
      activeWords.forEach((w, i) => {
        if (!wordSet.has(w.trim().toLowerCase())) bad.add(i);
      });
      setInvalidIndices(bad);
      setValidationError(
        bad.size > 0
          ? `${bad.size} word(s) are not valid BIP39 words. Check the highlighted fields.`
          : "Invalid recovery phrase. Please check the word order and try again."
      );
      return;
    }

    setLoading(true);
    try {
      await restoreWalletFromMnemonic(phrase);
      // Clear sensitive data from state before navigating
      setWords(Array(wordCount).fill(""));
      router.replace("/account-type");
    } catch (err) {
      setValidationError(
        err instanceof Error ? err.message : "Recovery failed. Please try again."
      );
    } finally {
      setLoading(false);
    }
  }, [words, wordCount, router]);

  // ── Derived state ─────────────────────────────────────────────────────────

  const filledCount = words.slice(0, wordCount).filter((w) => w.trim() !== "").length;
  const allFilled = filledCount === wordCount;

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <KeyboardAvoidingView
        style={{ flex: 1 }}
        behavior={Platform.OS === "ios" ? "padding" : undefined}
        keyboardVerticalOffset={0}
      >
        {/* Header */}
        <View style={styles.header}>
          <TouchableOpacity
            style={styles.backButton}
            onPress={() => router.back()}
            accessibilityLabel="Go back"
          >
            <Ionicons name="arrow-back" size={24} color={COLORS.black} />
          </TouchableOpacity>
          <Text style={styles.headerTitle}>Restore Wallet</Text>
          <View style={{ width: 40 }} />
        </View>

        <ScrollView
          style={styles.scroll}
          contentContainerStyle={styles.scrollContent}
          keyboardShouldPersistTaps="handled"
          showsVerticalScrollIndicator={false}
        >
          {/* Subtitle */}
          <Text style={styles.subtitle}>
            Enter your recovery phrase to restore your wallet. Make sure you're
            in a private location.
          </Text>

          {/* Word count tabs */}
          <View style={styles.tabRow}>
            <WordCountTab
              count={12}
              selected={wordCount === 12}
              onPress={() => handleWordCountChange(12)}
            />
            <WordCountTab
              count={24}
              selected={wordCount === 24}
              onPress={() => handleWordCountChange(24)}
            />
          </View>

          {/* Paste toggle */}
          <TouchableOpacity
            style={styles.pasteToggle}
            onPress={() => {
              setPasteMode((v) => !v);
              setValidationError(null);
            }}
            activeOpacity={0.8}
          >
            <Ionicons
              name={pasteMode ? "grid-outline" : "clipboard-outline"}
              size={16}
              color={COLORS.primary}
              style={{ marginRight: 6 }}
            />
            <Text style={styles.pasteToggleText}>
              {pasteMode ? "Enter words individually" : "Paste full phrase"}
            </Text>
          </TouchableOpacity>

          {pasteMode ? (
            /* ── Paste mode ── */
            <View style={styles.pasteContainer}>
              <TextInput
                style={styles.pasteInput}
                value={pasteText}
                onChangeText={(t) => {
                  setPasteText(t);
                  setValidationError(null);
                }}
                multiline
                autoCapitalize="none"
                autoCorrect={false}
                spellCheck={false}
                placeholder={`Paste your ${wordCount}-word recovery phrase here…`}
                placeholderTextColor="#BBBBBB"
                accessibilityLabel="Paste recovery phrase"
              />
              <Button
                title={`Apply ${wordCount} words`}
                onPress={handlePasteApply}
                variant="outline"
                style={{ marginTop: 12 }}
                disabled={pasteText.trim() === ""}
              />
            </View>
          ) : (
            /* ── Individual word inputs ── */
            <View style={styles.wordGrid}>
              {Array.from({ length: wordCount }, (_, i) => (
                <WordInput
                  key={i}
                  index={i}
                  value={words[i] ?? ""}
                  isInvalid={invalidIndices.has(i)}
                  onChangeText={(t) => handleWordChange(i, t)}
                  onSubmitEditing={() => {
                    const next = inputRefs.current[i + 1];
                    next?.current?.focus();
                  }}
                  inputRef={inputRefs.current[i]}
                />
              ))}
            </View>
          )}

          {/* Progress indicator */}
          {!pasteMode && (
            <Text style={styles.progressText}>
              {filledCount} / {wordCount} words entered
            </Text>
          )}

          {/* Validation error */}
          {validationError && (
            <View style={styles.errorBanner}>
              <Ionicons
                name="alert-circle-outline"
                size={18}
                color="#C53030"
                style={{ marginRight: 8, flexShrink: 0 }}
              />
              <Text style={styles.errorText}>{validationError}</Text>
            </View>
          )}

          {/* Security note */}
          <View style={styles.securityNote}>
            <Ionicons
              name="lock-closed-outline"
              size={16}
              color={COLORS.primary}
              style={{ marginRight: 8, flexShrink: 0 }}
            />
            <Text style={styles.securityNoteText}>
              Your phrase is processed entirely on-device and never sent to any
              server.
            </Text>
          </View>
        </ScrollView>

        {/* Footer */}
        <View style={styles.footer}>
          {loading ? (
            <View style={styles.loadingRow}>
              <ActivityIndicator color={COLORS.primary} />
              <Text style={styles.loadingText}>Restoring wallet…</Text>
            </View>
          ) : (
            <Button
              title="Restore Wallet"
              onPress={handleRecover}
              variant="primary"
              disabled={!allFilled && !pasteMode}
              style={!allFilled && !pasteMode ? styles.disabledButton : undefined}
            />
          )}
        </View>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────

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
  subtitle: {
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    lineHeight: 22,
    marginBottom: 20,
  },
  tabRow: {
    flexDirection: "row",
    backgroundColor: "#F5F5F5",
    borderRadius: 12,
    padding: 4,
    marginBottom: 16,
  },
  tab: {
    flex: 1,
    paddingVertical: 10,
    borderRadius: 10,
    alignItems: "center",
  },
  tabSelected: {
    backgroundColor: COLORS.white,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.08,
    shadowRadius: 4,
    elevation: 2,
  },
  tabText: {
    fontSize: 14,
    fontFamily: "Outfit_500Medium",
    color: "#888",
  },
  tabTextSelected: {
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
  },
  pasteToggle: {
    flexDirection: "row",
    alignItems: "center",
    alignSelf: "flex-end",
    marginBottom: 16,
    paddingVertical: 4,
  },
  pasteToggleText: {
    fontSize: 13,
    fontFamily: "Outfit_500Medium",
    color: COLORS.primary,
  },
  pasteContainer: {
    marginBottom: 16,
  },
  pasteInput: {
    backgroundColor: "#F7F7F7",
    borderRadius: 14,
    padding: 16,
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
    minHeight: 120,
    textAlignVertical: "top",
    borderWidth: 1,
    borderColor: "#E8E8E8",
    lineHeight: 24,
  },
  wordGrid: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 8,
    marginBottom: 12,
  },
  wordInputWrapper: {
    width: "48%",
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#F7F7F7",
    borderRadius: 10,
    borderWidth: 1,
    borderColor: "#EFEFEF",
    paddingHorizontal: 10,
    height: 44,
  },
  wordInputIndex: {
    fontSize: 11,
    fontFamily: "Outfit_500Medium",
    color: "#AAAAAA",
    minWidth: 20,
  },
  wordInput: {
    flex: 1,
    fontSize: 14,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
    paddingVertical: 0,
  },
  wordInputInvalid: {
    borderColor: "#FC8181",
    backgroundColor: "#FFF5F5",
  },
  progressText: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    textAlign: "center",
    marginBottom: 16,
  },
  errorBanner: {
    flexDirection: "row",
    alignItems: "flex-start",
    backgroundColor: "#FFF5F5",
    borderRadius: 12,
    padding: 14,
    marginBottom: 16,
    borderWidth: 1,
    borderColor: "#FEB2B2",
  },
  errorText: {
    flex: 1,
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#C53030",
    lineHeight: 19,
  },
  securityNote: {
    flexDirection: "row",
    alignItems: "flex-start",
    backgroundColor: "#F0FFF4",
    borderRadius: 12,
    padding: 14,
    borderWidth: 1,
    borderColor: COLORS.secondary,
  },
  securityNoteText: {
    flex: 1,
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: COLORS.primary,
    lineHeight: 19,
  },
  footer: {
    padding: 20,
    paddingBottom: 36,
  },
  loadingRow: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 12,
    height: 56,
  },
  loadingText: {
    fontSize: 16,
    fontFamily: "Outfit_500Medium",
    color: COLORS.primary,
  },
  disabledButton: {
    opacity: 0.5,
  },
});
