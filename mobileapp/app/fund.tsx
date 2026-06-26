import React, { useState, useEffect, useCallback, useRef } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  ActivityIndicator,
  TextInput,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter, Stack } from "expo-router";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { fetchWithRetry } from "../src/utils/retry";

const API_BASE = process.env.EXPO_PUBLIC_API_URL || "http://localhost:8080";

const NETWORKS = [
  { id: "solana", name: "Solana" },
  { id: "ethereum", name: "Ethereum" },
  { id: "bsc", name: "BNB Chain" },
  { id: "polygon", name: "Polygon" },
];

const TOKENS = [
  { id: "usdc", name: "USDC", symbol: "USDC" },
  { id: "usdt", name: "USDT", symbol: "USDT" },
  { id: "sol", name: "SOL", symbol: "SOL" },
  { id: "eth", name: "ETH", symbol: "ETH" },
];

interface BridgeQuote {
  fee: string;
  receiveAmount: string;
  route: string;
  estimatedTime: number;
  bridgeId: string;
}

interface BridgeStatus {
  step: number;
  message: string;
  confirmations: number;
  requiredConfirmations: number;
  completed: boolean;
  txHash?: string;
}

const BRIDGE_STEPS = [
  { key: "init", label: "Init" },
  { key: "lock", label: "Lock" },
  { key: "relay", label: "Relay" },
  { key: "receive", label: "Receive" },
];

export default function FundScreen() {
  const router = useRouter();
  const [selectedNetwork, setSelectedNetwork] = useState(NETWORKS[0]);
  const [selectedToken, setSelectedToken] = useState(TOKENS[0]);
  const [amount, setAmount] = useState("");
  const [loadingQuote, setLoadingQuote] = useState(false);
  const [quote, setQuote] = useState<BridgeQuote | null>(null);
  const [error, setError] = useState("");

  const [bridgeStep, setBridgeStep] = useState(0);
  const [stepMessage, setStepMessage] = useState("");
  const [bridgeStatus, setBridgeStatus] = useState<BridgeStatus | null>(null);
  const [bridgeId, setBridgeId] = useState<string | null>(null);
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchQuote = useCallback(async () => {
    if (!amount || parseFloat(amount) <= 0) return;
    setLoadingQuote(true);
    setError("");
    try {
      const res = await fetchWithRetry(`${API_BASE}/api/bridge/quote`, {
        method: "POST",
        body: JSON.stringify({
          sourceNetwork: selectedNetwork.id,
          sourceToken: selectedToken.symbol,
          amount,
          destinationNetwork: "stellar",
          destinationToken: "USDC",
        }),
      });
      const data: BridgeQuote = await res.json();
      setQuote(data);
    } catch {
      setError("Failed to fetch bridge quote. Please try again.");
    } finally {
      setLoadingQuote(false);
    }
  }, [amount, selectedNetwork.id, selectedToken.symbol]);

  useEffect(() => {
    setQuote(null);
    setError("");
    if (amount && parseFloat(amount) > 0) {
      const debounce = setTimeout(fetchQuote, 500);
      return () => clearTimeout(debounce);
    }
  }, [selectedNetwork.id, selectedToken.id, amount, fetchQuote]);

  const pollBridgeStatus = useCallback(async (id: string) => {
    try {
      const res = await fetchWithRetry(`${API_BASE}/api/bridge/status/${id}`);
      const data: BridgeStatus = await res.json();
      setBridgeStatus(data);
      setStepMessage(data.message);

      const stepIndex = BRIDGE_STEPS.findIndex(
        (s) => s.key === ["init", "lock", "relay", "receive"][data.step]
      );
      setBridgeStep(Math.min(stepIndex + 1, BRIDGE_STEPS.length));

      if (data.completed) {
        setBridgeStep(BRIDGE_STEPS.length);
        if (pollingRef.current) {
          clearInterval(pollingRef.current);
          pollingRef.current = null;
        }
      }
    } catch {
      // keep polling
    }
  }, []);

  const startBridge = async () => {
    if (!quote) return;
    setStepMessage("1. Initiating transfer...");
    setBridgeStep(1);
    try {
      const res = await fetchWithRetry(`${API_BASE}/api/bridge/initiate`, {
        method: "POST",
        body: JSON.stringify({
          sourceNetwork: selectedNetwork.id,
          sourceToken: selectedToken.symbol,
          amount,
          quoteId: quote.bridgeId,
        }),
      });
      const data = await res.json();
      const id = data.bridgeId || data.id;
      setBridgeId(id);
      setStepMessage(`2. Awaiting lock confirmations...`);

      pollingRef.current = setInterval(() => {
        pollBridgeStatus(id);
      }, 3000);
    } catch {
      setStepMessage("Bridge initiation failed. Please try again.");
      setBridgeStep(0);
    }
  };

  useEffect(() => {
    return () => {
      if (pollingRef.current) {
        clearInterval(pollingRef.current);
      }
    };
  }, []);

  const handleDone = () => {
    router.replace("/(personal)/home");
  };

  const getStepIndex = () => {
    if (!bridgeStatus) return Math.min(bridgeStep, BRIDGE_STEPS.length);
    return Math.min(bridgeStatus.step + 1, BRIDGE_STEPS.length);
  };

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Fund Wallet</Text>
        <View style={{ width: 40 }} />
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        {bridgeStep === 0 || (bridgeStep === 0 && !bridgeId) ? (
          <View>
            <Text style={styles.subtitle}>
              Bridge assets from other blockchains directly to your Stellar
              wallet using Allbridge Core.
            </Text>

            <Text style={styles.sectionLabel}>Source Network</Text>
            <ScrollView
              horizontal
              showsHorizontalScrollIndicator={false}
              style={styles.horizontalScroll}
            >
              {NETWORKS.map((network) => (
                <TouchableOpacity
                  key={network.id}
                  style={[
                    styles.networkCard,
                    selectedNetwork.id === network.id &&
                      styles.networkCardActive,
                  ]}
                  onPress={() => {
                    setSelectedNetwork(network);
                    setQuote(null);
                  }}
                >
                  <Ionicons
                    name="radio-button-off-outline"
                    size={20}
                    color={
                      selectedNetwork.id === network.id
                        ? COLORS.secondary
                        : COLORS.primary
                    }
                  />
                  <Text
                    style={[
                      styles.networkName,
                      selectedNetwork.id === network.id &&
                        styles.networkNameActive,
                    ]}
                  >
                    {network.name}
                  </Text>
                </TouchableOpacity>
              ))}
            </ScrollView>

            <Text style={styles.sectionLabel}>Token to Bridge</Text>
            <View style={styles.tokenGrid}>
              {TOKENS.map((token) => (
                <TouchableOpacity
                  key={token.id}
                  style={[
                    styles.tokenCard,
                    selectedToken.id === token.id && styles.tokenCardActive,
                  ]}
                  onPress={() => {
                    setSelectedToken(token);
                    setQuote(null);
                  }}
                >
                  <Text
                    style={[
                      styles.tokenText,
                      selectedToken.id === token.id && styles.tokenTextActive,
                    ]}
                  >
                    {token.name}
                  </Text>
                </TouchableOpacity>
              ))}
            </View>

            <Text style={styles.sectionLabel}>Amount</Text>
            <View style={styles.amountInputContainer}>
              <TextInput
                style={styles.amountInput}
                placeholder="0.00"
                keyboardType="numeric"
                value={amount}
                onChangeText={(val) => {
                  setAmount(val);
                  setQuote(null);
                }}
              />
              <Text style={styles.currencySuffix}>{selectedToken.symbol}</Text>
            </View>

            {error ? <Text style={styles.errorText}>{error}</Text> : null}

            {loadingQuote ? (
              <View style={styles.loadingQuoteContainer}>
                <ActivityIndicator color={COLORS.primary} />
                <Text style={styles.loadingQuoteText}>
                  Fetching bridge quote...
                </Text>
              </View>
            ) : quote ? (
              <View style={styles.quoteCard}>
                <Text style={styles.quoteTitle}>Allbridge Core Quote</Text>
                <View style={styles.quoteDivider} />

                <View style={styles.quoteRow}>
                  <Text style={styles.quoteLabel}>Bridge Route:</Text>
                  <Text style={styles.quoteVal}>{quote.route}</Text>
                </View>

                <View style={styles.quoteRow}>
                  <Text style={styles.quoteLabel}>Bridge fee:</Text>
                  <Text style={styles.quoteVal}>
                    {quote.fee} {selectedToken.symbol}
                  </Text>
                </View>

                <View style={styles.quoteRow}>
                  <Text style={styles.quoteLabel}>
                    Estimated USDC Received:
                  </Text>
                  <Text style={styles.quoteValHighlight}>
                    ${quote.receiveAmount} USDC
                  </Text>
                </View>

                <View style={styles.quoteRow}>
                  <Text style={styles.quoteLabel}>Est. Time:</Text>
                  <Text style={styles.quoteVal}>
                    ~{Math.ceil(quote.estimatedTime / 60)} min
                  </Text>
                </View>

                <Button
                  title="Confirm & Initiate Bridge"
                  onPress={startBridge}
                  style={[
                    styles.actionBtn,
                    { marginTop: 16, backgroundColor: "#2E7D32" },
                  ]}
                />
              </View>
            ) : null}
          </View>
        ) : (
          <View style={styles.stepperContainer}>
            <View style={styles.statusCircleOuter}>
              {bridgeStatus?.completed ? (
                <View style={styles.successCheck}>
                  <Ionicons name="checkmark" size={60} color="#1A4B4A" />
                </View>
              ) : (
                <ActivityIndicator
                  size="large"
                  color={COLORS.primary}
                  style={styles.spinner}
                />
              )}
            </View>

            <Text style={styles.stepperTitle}>
              {bridgeStatus?.completed
                ? "Bridging Successful!"
                : "Cross-Chain Deposit in Progress"}
            </Text>
            <Text style={styles.stepperDesc}>{stepMessage}</Text>

            <View style={styles.progressContainer}>
              {BRIDGE_STEPS.map((s, i) => {
                const currentStep = getStepIndex();
                const isActive = i < currentStep;
                const isLast = i === BRIDGE_STEPS.length - 1;
                return (
                  <React.Fragment key={s.key}>
                    <View
                      style={[
                        styles.progressDot,
                        isActive && styles.progressDotActive,
                      ]}
                    >
                      {isActive && (
                        <Ionicons
                          name="checkmark"
                          size={10}
                          color={COLORS.secondary}
                        />
                      )}
                    </View>
                    {!isLast && (
                      <View
                        style={[
                          styles.progressLine,
                          i < currentStep - 1 && styles.progressLineActive,
                        ]}
                      />
                    )}
                  </React.Fragment>
                );
              })}
            </View>

            <View style={styles.progressLabels}>
              {BRIDGE_STEPS.map((s, i) => {
                const currentStep = getStepIndex();
                return (
                  <Text
                    key={s.key}
                    style={[
                      styles.progressLabel,
                      i < currentStep && styles.progressLabelActive,
                      i === currentStep - 1 && bridgeStatus?.completed
                        ? styles.progressLabelActive
                        : null,
                    ]}
                  >
                    {i + 1}. {s.label}
                  </Text>
                );
              })}
            </View>

            {bridgeStatus?.txHash ? (
              <Text style={styles.txHash}>
                TX: {bridgeStatus.txHash.slice(0, 12)}...
              </Text>
            ) : null}

            {bridgeStatus?.completed && (
              <Button
                title="Back to Wallet"
                onPress={handleDone}
                style={[
                  styles.actionBtn,
                  { marginTop: 40, backgroundColor: COLORS.primary },
                ]}
              />
            )}
          </View>
        )}
      </ScrollView>
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
  scrollContent: {
    paddingHorizontal: 20,
    paddingTop: 10,
    paddingBottom: 40,
    flexGrow: 1,
  },
  subtitle: {
    fontSize: 14,
    color: "#666",
    marginBottom: 20,
    fontFamily: "Outfit_400Regular",
    lineHeight: 20,
  },
  sectionLabel: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginBottom: 10,
    marginTop: 10,
  },
  horizontalScroll: {
    marginBottom: 16,
  },
  networkCard: {
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#F5F5F5",
    paddingHorizontal: 16,
    paddingVertical: 12,
    borderRadius: 100,
    marginRight: 10,
    gap: 8,
    borderWidth: 1,
    borderColor: "#E0E0E0",
  },
  networkCardActive: {
    backgroundColor: COLORS.primary,
    borderColor: COLORS.primary,
  },
  networkName: {
    fontSize: 14,
    color: COLORS.primary,
    fontFamily: "Outfit_500Medium",
  },
  networkNameActive: {
    color: COLORS.secondary,
    fontFamily: "Outfit_700Bold",
  },
  tokenGrid: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 10,
    marginBottom: 20,
  },
  tokenCard: {
    flex: 1,
    minWidth: "22%",
    height: 44,
    backgroundColor: "#F5F5F5",
    borderRadius: 22,
    justifyContent: "center",
    alignItems: "center",
    borderWidth: 1,
    borderColor: "#E0E0E0",
  },
  tokenCardActive: {
    backgroundColor: COLORS.primary,
    borderColor: COLORS.primary,
  },
  tokenText: {
    fontSize: 14,
    color: COLORS.primary,
    fontFamily: "Outfit_600SemiBold",
  },
  tokenTextActive: {
    color: COLORS.secondary,
    fontFamily: "Outfit_700Bold",
  },
  amountInputContainer: {
    flexDirection: "row",
    alignItems: "center",
    borderWidth: 1,
    borderColor: "#E0E0E0",
    borderRadius: 12,
    paddingHorizontal: 16,
    height: 56,
    marginBottom: 12,
    backgroundColor: "#FDFDFD",
  },
  amountInput: {
    flex: 1,
    fontSize: 18,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
  },
  currencySuffix: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  errorText: {
    fontSize: 13,
    color: "#CC0000",
    fontFamily: "Outfit_500Medium",
    marginBottom: 12,
    textAlign: "center",
  },
  loadingQuoteContainer: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 10,
    marginVertical: 20,
  },
  loadingQuoteText: {
    fontSize: 14,
    color: "#666",
    fontFamily: "Outfit_500Medium",
  },
  actionBtn: {
    marginTop: 10,
    height: 56,
    borderRadius: 28,
  },
  quoteCard: {
    backgroundColor: "#FAFAFA",
    borderRadius: 16,
    padding: 16,
    borderWidth: 1,
    borderColor: "#ECECEC",
    marginTop: 10,
  },
  quoteTitle: {
    fontSize: 15,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    marginBottom: 8,
  },
  quoteDivider: {
    height: 1,
    backgroundColor: "#EAEAEA",
    marginBottom: 12,
  },
  quoteRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    marginBottom: 8,
  },
  quoteLabel: {
    fontSize: 13,
    color: "#666",
    fontFamily: "Outfit_400Regular",
  },
  quoteVal: {
    fontSize: 13,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
  },
  quoteValHighlight: {
    fontSize: 14,
    fontFamily: "Outfit_700Bold",
    color: "#2E7D32",
  },
  stepperContainer: {
    alignItems: "center",
    paddingTop: 40,
  },
  statusCircleOuter: {
    width: 140,
    height: 140,
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 24,
  },
  spinner: {
    transform: [{ scale: 1.5 }],
  },
  successCheck: {
    width: 100,
    height: 100,
    borderRadius: 50,
    borderWidth: 4,
    borderColor: "#1A4B4A",
    justifyContent: "center",
    alignItems: "center",
    backgroundColor: COLORS.white,
  },
  stepperTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 10,
  },
  stepperDesc: {
    fontSize: 14,
    color: "#555",
    textAlign: "center",
    fontFamily: "Outfit_400Regular",
    marginBottom: 40,
    paddingHorizontal: 20,
  },
  progressContainer: {
    flexDirection: "row",
    alignItems: "center",
    width: "70%",
    justifyContent: "center",
  },
  progressDot: {
    width: 20,
    height: 20,
    borderRadius: 10,
    backgroundColor: "#E0E0E0",
    justifyContent: "center",
    alignItems: "center",
  },
  progressDotActive: {
    backgroundColor: COLORS.primary,
  },
  progressLine: {
    flex: 1,
    height: 3,
    backgroundColor: "#E0E0E0",
  },
  progressLineActive: {
    backgroundColor: COLORS.primary,
  },
  progressLabels: {
    flexDirection: "row",
    width: "80%",
    justifyContent: "space-between",
    marginTop: 10,
  },
  progressLabel: {
    fontSize: 12,
    color: "#999",
    fontFamily: "Outfit_500Medium",
    width: 60,
    textAlign: "center",
  },
  progressLabelActive: {
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
  },
  txHash: {
    fontSize: 11,
    color: "#999",
    fontFamily: "Outfit_400Regular",
    marginTop: 16,
  },
});
