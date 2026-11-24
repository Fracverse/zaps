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
import { getLocalKeypair } from "../src/services/stellarWallet";

const API_BASE = process.env.EXPO_PUBLIC_API_URL || "http://localhost:8080";

const NETWORKS = [
  { id: "SOL", name: "Solana" },
  { id: "ETH", name: "Ethereum" },
  { id: "BSC", name: "BNB Chain" },
  { id: "POL", name: "Polygon" },
];

const TOKENS = [
  { id: "usdc", name: "USDC", symbol: "USDC", decimals: 6 },
  { id: "usdt", name: "USDT", symbol: "USDT", decimals: 6 },
  { id: "sol", name: "SOL", symbol: "SOL", decimals: 9 },
  { id: "eth", name: "ETH", symbol: "ETH", decimals: 18 },
];

interface BridgeQuote {
  fee: string;
  receive_amount: string;
  bridge_tx_data: string;
  route?: string;
  bridgeId?: string;
}

interface DropdownOption {
  id: string;
  name: string;
}

interface DropdownSelectorProps {
  label: string;
  value: DropdownOption;
  options: DropdownOption[];
  onChange: (option: DropdownOption) => void;
}

function DropdownSelector({
  label,
  value,
  options,
  onChange,
}: DropdownSelectorProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <View style={styles.selectorContainer}>
      <Text style={styles.sectionLabel}>{label}</Text>
      <TouchableOpacity
        style={styles.selectorButton}
        onPress={() => setExpanded((current) => !current)}
        accessibilityRole="button"
        accessibilityLabel={`${label}: ${value.name}`}
        accessibilityState={{ expanded }}
      >
        <Text style={styles.selectorValue}>{value.name}</Text>
        <Ionicons
          name={expanded ? "chevron-up" : "chevron-down"}
          size={18}
          color={COLORS.primary}
        />
      </TouchableOpacity>
      {expanded ? (
        <View style={styles.selectorOptions}>
          {options.map((option) => (
            <TouchableOpacity
              key={option.id}
              style={[
                styles.selectorOption,
                option.id === value.id && styles.selectorOptionActive,
              ]}
              onPress={() => {
                onChange(option);
                setExpanded(false);
              }}
              accessibilityRole="button"
              accessibilityState={{ selected: option.id === value.id }}
            >
              <Text
                style={[
                  styles.selectorOptionText,
                  option.id === value.id && styles.selectorOptionTextActive,
                ]}
              >
                {option.name}
              </Text>
              {option.id === value.id ? (
                <Ionicons name="checkmark" size={18} color={COLORS.primary} />
              ) : null}
            </TouchableOpacity>
          ))}
        </View>
      ) : null}
    </View>
  );
}

function toBaseUnits(amount: string, decimals: number): string | null {
  const match = /^(\d+)(?:\.(\d*))?$/.exec(amount.trim());
  if (!match || (match[2]?.length ?? 0) > decimals) return null;

  const units = `${match[1]}${(match[2] ?? "").padEnd(decimals, "0")}`;
  const normalized = units.replace(/^0+(?=\d)/, "");
  return /^0+$/.test(normalized) ? null : normalized;
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
  const [destinationAddress, setDestinationAddress] = useState("");
  const [loadingQuote, setLoadingQuote] = useState(false);
  const [quote, setQuote] = useState<BridgeQuote | null>(null);
  const [error, setError] = useState("");

  const [bridgeStep, setBridgeStep] = useState(0);
  const [stepMessage, setStepMessage] = useState("");
  const [bridgeStatus, setBridgeStatus] = useState<BridgeStatus | null>(null);
  const [bridgeId, setBridgeId] = useState<string | null>(null);
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const quoteRequestRef = useRef(0);

  useEffect(() => {
    let isMounted = true;
    getLocalKeypair().then((keypair) => {
      if (isMounted && keypair) setDestinationAddress(keypair.publicKey());
    });
    return () => {
      isMounted = false;
    };
  }, []);

  const fetchQuote = useCallback(
    async (requestId: number) => {
      const amountInBaseUnits = toBaseUnits(amount, selectedToken.decimals);
      if (!amountInBaseUnits) return;
      if (!destinationAddress.trim()) {
        setError("Enter a Stellar destination address to get a quote.");
        return;
      }

      setLoadingQuote(true);
      setError("");
      try {
        const res = await fetchWithRetry(`${API_BASE}/api/bridge/quote`, {
          method: "POST",
          body: JSON.stringify({
            source_chain: selectedNetwork.id,
            source_token: selectedToken.symbol,
            amount: amountInBaseUnits,
            destination_chain: "STLR",
            destination_token: "USDC",
            destination_address: destinationAddress.trim(),
          }),
        });
        const data = (await res.json()) as BridgeQuote;
        if (!data.fee || !data.receive_amount) {
          throw new Error("The bridge provider returned an incomplete quote.");
        }
        if (requestId !== quoteRequestRef.current) return;
        setQuote(data);
      } catch (quoteError) {
        if (requestId !== quoteRequestRef.current) return;
        setError(
          quoteError instanceof Error
            ? quoteError.message
            : "Failed to fetch bridge quote. Please try again."
        );
      } finally {
        if (requestId === quoteRequestRef.current) setLoadingQuote(false);
      }
    },
    [amount, destinationAddress, selectedNetwork.id, selectedToken]
  );

  useEffect(() => {
    const requestId = ++quoteRequestRef.current;
    setQuote(null);
    setError("");
    setLoadingQuote(false);
    if (toBaseUnits(amount, selectedToken.decimals)) {
      const debounce = setTimeout(() => {
        void fetchQuote(requestId);
      }, 500);
      return () => clearTimeout(debounce);
    }
  }, [
    selectedNetwork.id,
    selectedToken,
    amount,
    destinationAddress,
    fetchQuote,
  ]);

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

            <DropdownSelector
              label="Source Network"
              value={selectedNetwork}
              options={NETWORKS}
              onChange={(option) => {
                const network = NETWORKS.find((item) => item.id === option.id);
                if (network) setSelectedNetwork(network);
              }}
            />

            <DropdownSelector
              label="Token to Bridge"
              value={selectedToken}
              options={TOKENS}
              onChange={(option) => {
                const token = TOKENS.find((item) => item.id === option.id);
                if (token) setSelectedToken(token);
              }}
            />

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

            <Text style={styles.sectionLabel}>Stellar Destination</Text>
            <View style={styles.amountInputContainer}>
              <TextInput
                style={styles.amountInput}
                placeholder="Enter Stellar public key"
                autoCapitalize="characters"
                value={destinationAddress}
                onChangeText={setDestinationAddress}
                accessibilityLabel="Stellar destination address"
              />
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
                  <Text style={styles.quoteVal}>
                    {quote.route ?? `${selectedNetwork.name} to Stellar`}
                  </Text>
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
                    {quote.receive_amount} USDC
                  </Text>
                </View>

                <Button
                  title="Confirm & Initiate Bridge"
                  onPress={startBridge}
                  style={{
                    ...styles.actionBtn,
                    marginTop: 16,
                    backgroundColor: "#2E7D32",
                  }}
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
                const isStepCompleted =
                  i < currentStep - 1 ||
                  (i === currentStep - 1 && bridgeStatus?.completed);
                const isCurrent =
                  i === currentStep - 1 && !bridgeStatus?.completed;
                const isLast = i === BRIDGE_STEPS.length - 1;

                return (
                  <React.Fragment key={s.key}>
                    <View
                      style={[
                        styles.progressDot,
                        isStepCompleted && styles.progressDotActive,
                        isCurrent && styles.progressDotCurrent,
                      ]}
                    >
                      {isStepCompleted ? (
                        <Ionicons
                          name="checkmark"
                          size={12}
                          color={COLORS.secondary}
                        />
                      ) : isCurrent ? (
                        <ActivityIndicator
                          size="small"
                          color={COLORS.secondary}
                          style={{ transform: [{ scale: 0.6 }] }}
                        />
                      ) : null}
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
                const isStepCompleted =
                  i < currentStep - 1 ||
                  (i === currentStep - 1 && bridgeStatus?.completed);
                const isCurrent =
                  i === currentStep - 1 && !bridgeStatus?.completed;
                return (
                  <Text
                    key={s.key}
                    style={[
                      styles.progressLabel,
                      (isStepCompleted || isCurrent) &&
                        styles.progressLabelActive,
                    ]}
                  >
                    {i + 1}. {s.label}
                  </Text>
                );
              })}
            </View>

            {/* Dynamic polling confirmations badge & progress bar */}
            {bridgeStatus &&
              bridgeStatus.requiredConfirmations > 0 &&
              !bridgeStatus.completed && (
                <View style={styles.confirmationBox}>
                  <View style={styles.confirmationHeader}>
                    <View style={styles.confirmationPulseDot} />
                    <Text style={styles.confirmationLabel}>
                      Lock Confirmations
                    </Text>
                    <Text style={styles.confirmationCount}>
                      {bridgeStatus.confirmations} /{" "}
                      {bridgeStatus.requiredConfirmations}
                    </Text>
                  </View>
                  <View style={styles.confirmationProgressBar}>
                    <View
                      style={[
                        styles.confirmationProgressFill,
                        {
                          width: `${Math.min(
                            100,
                            Math.max(
                              5,
                              Math.round(
                                (bridgeStatus.confirmations /
                                  bridgeStatus.requiredConfirmations) *
                                  100
                              )
                            )
                          )}%`,
                        },
                      ]}
                    />
                  </View>
                </View>
              )}

            {bridgeStatus?.txHash ? (
              <Text style={styles.txHash}>
                TX: {bridgeStatus.txHash.slice(0, 12)}...
              </Text>
            ) : null}

            {bridgeStatus?.completed && (
              <Button
                title="Back to Wallet"
                onPress={handleDone}
                style={{
                  ...styles.actionBtn,
                  marginTop: 40,
                  backgroundColor: COLORS.primary,
                }}
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
  selectorContainer: {
    marginBottom: 8,
  },
  selectorButton: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    backgroundColor: COLORS.white,
    height: 52,
    paddingHorizontal: 16,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "#DDE5DF",
  },
  selectorValue: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
  },
  selectorOptions: {
    marginTop: 6,
    backgroundColor: COLORS.white,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "#DDE5DF",
    overflow: "hidden",
  },
  selectorOption: {
    minHeight: 48,
    paddingHorizontal: 16,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    borderBottomWidth: 1,
    borderBottomColor: "#EEF2EF",
  },
  selectorOptionActive: {
    backgroundColor: "#F0F7F2",
  },
  selectorOptionText: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
  },
  selectorOptionTextActive: {
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
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
  progressDotCurrent: {
    backgroundColor: COLORS.primary,
    borderWidth: 2,
    borderColor: "#4ADE80",
    transform: [{ scale: 1.15 }],
  },
  confirmationBox: {
    width: "82%",
    backgroundColor: "#F4FAF6",
    borderRadius: 14,
    padding: 14,
    marginTop: 24,
    borderWidth: 1,
    borderColor: "#D1E7DD",
  },
  confirmationHeader: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    marginBottom: 8,
  },
  confirmationPulseDot: {
    width: 8,
    height: 8,
    borderRadius: 4,
    backgroundColor: "#16A34A",
    marginRight: 6,
  },
  confirmationLabel: {
    flex: 1,
    fontSize: 13,
    fontFamily: "Outfit_600SemiBold",
    color: "#166534",
  },
  confirmationCount: {
    fontSize: 13,
    fontFamily: "Outfit_700Bold",
    color: "#166534",
  },
  confirmationProgressBar: {
    height: 6,
    backgroundColor: "#E2E8F0",
    borderRadius: 3,
    overflow: "hidden",
  },
  confirmationProgressFill: {
    height: "100%",
    backgroundColor: "#16A34A",
    borderRadius: 3,
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
