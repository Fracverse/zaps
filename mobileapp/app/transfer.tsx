import React, { useState, useEffect, useCallback, useRef } from "react";
import { ErrorBoundary } from "../src/components/ErrorBoundary";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  LayoutAnimation,
  Platform,
  UIManager,
  Alert,
  ActivityIndicator,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter, Stack } from "expo-router";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Input } from "../src/components/Input";
import { AccountTypeCard } from "../src/components/AccountTypeCard";
import AsyncStorage from "@react-native-async-storage/async-storage";
import {
  checkFreighter,
  connectFreighter,
  connectAlbedo,
  connectLocalWallet,
  generateLocalKeypair,
  saveLocalKeypair,
  submitPayment,
  getLocalKeypair,
  StellarWalletState,
} from "../src/services/stellarWallet";

import ZapsIcon from "../assets/icon-4.svg";
import WalletIcon from "../assets/wallet.svg";
import XLMLogo from "../assets/XML-logo.svg";
import USDTLogo from "../assets/USDT-logo.svg";
import USDCLogo from "../assets/USDC-logo.svg";

if (
  Platform.OS === "android" &&
  UIManager.setLayoutAnimationEnabledExperimental
) {
  UIManager.setLayoutAnimationEnabledExperimental(true);
}

const STELLAR_ASSET_ISSUERS: Record<string, string | undefined> = {
  USDC: "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
  USDT: "GCQTGZQQ5G4PTM2GL7CDIFKUBIPEC52BROAQIAPW53XBRJVN6ZJVTG6V",
};

const TOKENS = [
  {
    id: "xlm",
    symbol: "XLM",
    name: "Stellar",
    balance: "100.00",
    value: "125.32",
    Icon: XLMLogo,
  },
  {
    id: "usdt",
    symbol: "USDT",
    name: "Tether",
    balance: "100.00",
    value: "100",
    Icon: USDTLogo,
  },
  {
    id: "usdc",
    symbol: "USDC",
    name: "USD Coin",
    balance: "100.00",
    value: "100",
    Icon: USDCLogo,
  },
];

const TokenSelectCard = ({
  symbol,
  balance,
  value,
  Icon,
  selected,
  onPress,
}: any) => (
  <TouchableOpacity
    style={[styles.tokenCard, selected && styles.tokenCardSelected]}
    onPress={onPress}
    activeOpacity={0.8}
  >
    <View style={styles.tokenIcon}>
      <Icon width={32} height={32} />
    </View>
    <View style={styles.tokenInfo}>
      <Text style={styles.tokenSymbol}>{symbol}</Text>
      <Text style={styles.tokenBalance}>{balance}</Text>
    </View>
    <Text style={styles.tokenValue}>${value}</Text>
  </TouchableOpacity>
);

const API_BASE =
  (typeof process !== "undefined" && process.env?.EXPO_PUBLIC_API_URL) ||
  "http://localhost:8080";

interface ZapsUser {
  username: string;
  address: string;
  avatar_url: string | null;
}

function TransferScreen() {
  const router = useRouter();
  const [step, setStep] = useState(0);
  const [transferType, setTransferType] = useState<"ZAPS" | "external" | null>(
    "ZAPS"
  );
  const [recipient, setRecipient] = useState("");
  const [amount, setAmount] = useState("");
  const [description, setDescription] = useState("");
  const [visibility, setVisibility] = useState<
    "PUBLIC" | "FRIENDS" | "PRIVATE"
  >("PUBLIC");
  const [selectedToken, setSelectedToken] = useState(TOKENS[0].id);
  const [walletState, setWalletState] = useState<StellarWalletState | null>(
    null
  );
  const [connecting, setConnecting] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  // Recipient search state
  const [searchResults, setSearchResults] = useState<ZapsUser[]>([]);
  const [searching, setSearching] = useState(false);
  const [showDropdown, setShowDropdown] = useState(false);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const searchUsers = useCallback(async (query: string) => {
    if (!query || query.length < 2) {
      setSearchResults([]);
      setShowDropdown(false);
      return;
    }
    setSearching(true);
    try {
      const res = await fetch(
        `${API_BASE}/api/users/search?q=${encodeURIComponent(query)}&limit=6`
      );
      if (!res.ok) throw new Error("Search failed");
      const data: ZapsUser[] = await res.json();
      setSearchResults(data);
      setShowDropdown(data.length > 0);
    } catch {
      setSearchResults([]);
      setShowDropdown(false);
    } finally {
      setSearching(false);
    }
  }, []);

  // Debounce search while user types (ZAPS mode only)
  useEffect(() => {
    if (transferType !== "ZAPS") return;
    if (searchTimer.current) clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(() => {
      searchUsers(recipient);
    }, 350);
    return () => {
      if (searchTimer.current) clearTimeout(searchTimer.current);
    };
  }, [recipient, transferType, searchUsers]);

  const handleSelectUser = useCallback((user: ZapsUser) => {
    setRecipient(user.username);
    setSearchResults([]);
    setShowDropdown(false);
  }, []);

  const token = TOKENS.find((t) => t.id === selectedToken) || TOKENS[0];

  useEffect(() => {
    (async () => {
      const kp = await getLocalKeypair();
      if (kp) {
        setWalletState({
          publicKey: kp.publicKey(),
          isConnected: true,
          source: "local",
        });
      }
    })();
  }, []);

  const handleConnectWallet = useCallback(
    async (source: "freighter" | "albedo" | "local") => {
      setConnecting(true);
      try {
        let state: StellarWalletState;
        if (source === "freighter") {
          const hasFreighter = await checkFreighter();
          if (!hasFreighter) {
            Alert.alert(
              "Freighter Not Found",
              "Please install the Freighter wallet extension."
            );
            setConnecting(false);
            return;
          }
          state = await connectFreighter();
        } else if (source === "albedo") {
          state = await connectAlbedo();
        } else {
          const existing = await getLocalKeypair();
          if (existing) {
            state = await connectLocalWallet();
          } else {
            const kp = generateLocalKeypair();
            await saveLocalKeypair(kp.secretKey);
            state = {
              publicKey: kp.publicKey,
              isConnected: true,
              source: "local",
            };
          }
        }
        setWalletState(state);
      } catch (e) {
        Alert.alert("Connection Failed", (e as Error).message);
      } finally {
        setConnecting(false);
      }
    },
    []
  );

  const handleNext = async () => {
    if (step === 2) {
      if (!walletState?.isConnected) {
        setStep(4);
        return;
      }
      setSubmitting(true);
      try {
        const assetIssuer =
          token.symbol !== "XLM"
            ? STELLAR_ASSET_ISSUERS[token.symbol]
            : undefined;
        const result = await submitPayment(
          recipient,
          amount,
          walletState.source!,
          walletState.publicKey,
          token.symbol,
          assetIssuer,
          description || undefined
        );
        const stored = await AsyncStorage.getItem("pending_transfers");
        const list = stored ? JSON.parse(stored) : [];
        list.unshift({
          recipient,
          amount,
          description: description || "Sent money",
          visibility,
          token: token.symbol,
          hash: result.hash,
        });
        await AsyncStorage.setItem("pending_transfers", JSON.stringify(list));
      } catch (e) {
        Alert.alert("Transfer Failed", (e as Error).message);
        setSubmitting(false);
        return;
      }
      setSubmitting(false);
    }

    if (step === 3) {
      router.replace("/(personal)/home");
      return;
    }

    LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
    setStep(step + 1);
  };

  const handleBack = () => {
    if (step === 0) {
      router.back();
    } else if (step === 3) {
      router.replace("/(personal)/home");
    } else if (step === 4) {
      LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
      setStep(2);
    } else {
      LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
      setStep(step - 1);
    }
  };

  const handleDisconnect = async () => {
    setWalletState(null);
    setStep(2);
  };

  const renderStep0 = () => (
    <View style={styles.stepContainer}>
      <Text style={styles.subtitle}>Choose how you want to send money.</Text>
      <View style={styles.cardsContainer}>
        <AccountTypeCard
          title="Zaps User"
          description="Send instantly to any Zaps user via their ZAPS ID"
          Icon={ZapsIcon}
          selected={transferType === "ZAPS"}
          onPress={() => setTransferType("ZAPS")}
        />
        <AccountTypeCard
          title="External Wallet"
          description="Send to any XLM or Stellar compatible wallet address"
          Icon={WalletIcon}
          selected={transferType === "external"}
          onPress={() => setTransferType("external")}
        />
      </View>
    </View>
  );

  const renderStep1 = () => (
    <View style={styles.stepContainer}>
      <View style={styles.inputsSection}>
        {/* Recipient input + live-search dropdown (ZAPS mode only) */}
        <View style={styles.recipientSearchWrapper}>
          <Input
            placeholder={
              transferType === "ZAPS"
                ? "Recipient ZAPS ID (e.g. tolu.zaps)"
                : "Wallet Address"
            }
            value={recipient}
            onChangeText={(text: string) => {
              setRecipient(text);
              if (transferType !== "ZAPS") return;
              if (text.length < 2) {
                setShowDropdown(false);
                setSearchResults([]);
              }
            }}
            autoCapitalize="none"
            style={styles.transferInput}
          />
          {/* Searching indicator */}
          {transferType === "ZAPS" && searching && (
            <ActivityIndicator
              size="small"
              color="#1A4B4A"
              style={styles.searchingIndicator}
            />
          )}
          {/* Dropdown results */}
          {transferType === "ZAPS" &&
            showDropdown &&
            searchResults.length > 0 && (
              <View style={styles.dropdownContainer}>
                {searchResults.map((user) => (
                  <TouchableOpacity
                    key={user.address}
                    style={styles.dropdownItem}
                    onPress={() => handleSelectUser(user)}
                    activeOpacity={0.75}
                  >
                    <View style={styles.dropdownAvatar}>
                      <Text style={styles.dropdownAvatarText}>
                        {user.username.charAt(0).toUpperCase()}
                      </Text>
                    </View>
                    <View style={styles.dropdownInfo}>
                      <Text style={styles.dropdownUsername}>
                        {user.username}
                      </Text>
                      <Text style={styles.dropdownAddress} numberOfLines={1}>
                        {user.address.slice(0, 10)}…{user.address.slice(-6)}
                      </Text>
                    </View>
                    <Ionicons
                      name="chevron-forward"
                      size={16}
                      color="#BDBDBD"
                    />
                  </TouchableOpacity>
                ))}
              </View>
            )}
        </View>

        {/* Custom Amount Display */}
        <TouchableOpacity
          activeOpacity={1}
          style={[styles.transferInput, styles.amountDisplayContainer]}
        >
          <Text style={styles.nairaSymbol}>₦</Text>
          <Text style={styles.amountText}>{amount || "0"}</Text>
        </TouchableOpacity>

        {/* Custom Numeric Keypad */}
        <View style={styles.keypadContainer}>
          <View style={styles.keypadRow}>
            {["1", "2", "3"].map((num) => (
              <TouchableOpacity
                key={num}
                style={styles.keypadButton}
                onPress={() => setAmount((prev: string) => prev + num)}
              >
                <Text style={styles.keypadButtonText}>{num}</Text>
              </TouchableOpacity>
            ))}
          </View>
          <View style={styles.keypadRow}>
            {["4", "5", "6"].map((num) => (
              <TouchableOpacity
                key={num}
                style={styles.keypadButton}
                onPress={() => setAmount((prev: string) => prev + num)}
              >
                <Text style={styles.keypadButtonText}>{num}</Text>
              </TouchableOpacity>
            ))}
          </View>
          <View style={styles.keypadRow}>
            {["7", "8", "9"].map((num) => (
              <TouchableOpacity
                key={num}
                style={styles.keypadButton}
                onPress={() => setAmount((prev: string) => prev + num)}
              >
                <Text style={styles.keypadButtonText}>{num}</Text>
              </TouchableOpacity>
            ))}
          </View>
          <View style={styles.keypadRow}>
            <TouchableOpacity
              style={styles.keypadButton}
              onPress={() => setAmount((prev: string) => prev + ".")}
            >
              <Text style={styles.keypadButtonText}>.</Text>
            </TouchableOpacity>
            <TouchableOpacity
              style={styles.keypadButton}
              onPress={() => setAmount((prev: string) => prev + "0")}
            >
              <Text style={styles.keypadButtonText}>0</Text>
            </TouchableOpacity>
            <TouchableOpacity
              style={styles.keypadButton}
              onPress={() => setAmount((prev: string) => prev.slice(0, -1))}
            >
              <Text style={styles.keypadButtonText}>⌫</Text>
            </TouchableOpacity>
          </View>
        </View>

        <Input
          placeholder="What is this for? (e.g. Lunch 🍕)"
          value={description}
          onChangeText={setDescription}
          maxLength={100}
          style={styles.transferInput}
        />
      </View>

      {/* Visibility Selector */}
      <View style={styles.visibilitySection}>
        <Text style={styles.sectionLabel}>Who can see this payment?</Text>
        <View style={styles.visibilityOptions}>
          <TouchableOpacity
            style={[
              styles.visibilityBtn,
              visibility === "PUBLIC" && styles.visibilityBtnActive,
            ]}
            onPress={() => setVisibility("PUBLIC")}
          >
            <Ionicons
              name="globe-outline"
              size={18}
              color={visibility === "PUBLIC" ? COLORS.secondary : "#666"}
            />
            <Text
              style={[
                styles.visibilityText,
                visibility === "PUBLIC" && styles.visibilityTextActive,
              ]}
            >
              Public
            </Text>
          </TouchableOpacity>

          <TouchableOpacity
            style={[
              styles.visibilityBtn,
              visibility === "FRIENDS" && styles.visibilityBtnActive,
            ]}
            onPress={() => setVisibility("FRIENDS")}
          >
            <Ionicons
              name="people-outline"
              size={18}
              color={visibility === "FRIENDS" ? COLORS.secondary : "#666"}
            />
            <Text
              style={[
                styles.visibilityText,
                visibility === "FRIENDS" && styles.visibilityTextActive,
              ]}
            >
              Friends
            </Text>
          </TouchableOpacity>

          <TouchableOpacity
            style={[
              styles.visibilityBtn,
              visibility === "PRIVATE" && styles.visibilityBtnActive,
            ]}
            onPress={() => setVisibility("PRIVATE")}
          >
            <Ionicons
              name="lock-closed-outline"
              size={18}
              color={visibility === "PRIVATE" ? COLORS.secondary : "#666"}
            />
            <Text
              style={[
                styles.visibilityText,
                visibility === "PRIVATE" && styles.visibilityTextActive,
              ]}
            >
              Private
            </Text>
          </TouchableOpacity>
        </View>
        <Text style={styles.visibilityDesc}>
          {visibility === "PUBLIC" && "Visible to anyone on the Zaps network."}
          {visibility === "FRIENDS" && "Visible only to you and your friends."}
          {visibility === "PRIVATE" && "Visible only to you and the recipient."}
        </Text>
      </View>

      <View style={styles.payWithSection}>
        <Text style={styles.payWithLabel}>Pay with</Text>
        <View style={styles.tokenList}>
          {TOKENS.map((token) => (
            <TokenSelectCard
              key={token.id}
              {...token}
              selected={selectedToken === token.id}
              onPress={() => setSelectedToken(token.id)}
            />
          ))}
        </View>
      </View>
    </View>
  );

  const renderStep2 = () => (
    <View style={styles.stepContainer}>
      <View style={styles.summaryCardLarge}>
        <View style={styles.summaryIconLarge}>
          <token.Icon width={60} height={60} />
        </View>
        <Text style={styles.summaryAmountText}>
          ₦{amount} (via {token.symbol})
        </Text>
        <Text style={styles.summaryFiatText}>Social payment on Stellar</Text>

        <View style={styles.divider} />

        <View style={styles.infoRow}>
          <View style={styles.recipientBadge}>
            <ZapsIcon width={16} height={16} />
          </View>
          <View style={styles.infoCol}>
            <Text style={styles.infoLabel}>Recipient</Text>
            <Text style={styles.infoValue}>{recipient}</Text>
          </View>
        </View>

        <View style={[styles.infoRow, { marginTop: 16 }]}>
          <View style={styles.recipientBadge}>
            <Ionicons name="chatbubble-outline" size={16} color="#777" />
          </View>
          <View style={styles.infoCol}>
            <Text style={styles.infoLabel}>Note</Text>
            <Text style={styles.infoValue}>{description || "No note"}</Text>
          </View>
        </View>

        <View style={[styles.infoRow, { marginTop: 16 }]}>
          <View style={styles.recipientBadge}>
            <Ionicons name="eye-outline" size={16} color="#777" />
          </View>
          <View style={styles.infoCol}>
            <Text style={styles.infoLabel}>Privacy</Text>
            <Text style={styles.infoValue}>{visibility}</Text>
          </View>
        </View>
      </View>
    </View>
  );

  const renderStep3 = () => (
    <View style={[styles.stepContainer, styles.centerContent]}>
      <View style={styles.successOuter}>
        <View
          style={[
            styles.successRing,
            { width: 220, height: 220, opacity: 0.4 },
          ]}
        />
        <View
          style={[
            styles.successRing,
            { width: 180, height: 180, opacity: 0.4 },
          ]}
        />
        <View style={styles.successCheck}>
          <Ionicons name="checkmark" size={60} color="#1A4B4A" />
        </View>
      </View>

      <Text style={styles.successTitle}>Transfer Successful</Text>

      <View style={styles.amountCapsule}>
        <Text style={styles.amountCapsuleText}>₦{amount}</Text>
      </View>
    </View>
  );

  const renderStep4 = () => (
    <View style={styles.stepContainer}>
      <Text style={styles.subtitle}>
        Connect a Stellar wallet to authorize this transfer.
      </Text>

      {walletState?.isConnected ? (
        <View style={styles.connectedCard}>
          <View style={styles.connectedIcon}>
            <Ionicons
              name="wallet-outline"
              size={32}
              color={COLORS.secondary}
            />
          </View>
          <Text style={styles.connectedTitle}>Wallet Connected</Text>
          <Text style={styles.connectedAddress}>
            {walletState.publicKey.slice(0, 8)}...
            {walletState.publicKey.slice(-6)}
          </Text>
          <Text style={styles.connectedSource}>via {walletState.source}</Text>
          <View style={styles.connectedActions}>
            <Button
              title="Continue with this wallet"
              onPress={() => {
                setStep(2);
                handleNext();
              }}
              loading={submitting}
              style={{ backgroundColor: "#1A4B4A", marginTop: 16 }}
            />
            <TouchableOpacity
              onPress={handleDisconnect}
              style={styles.disconnectBtn}
            >
              <Text style={styles.disconnectText}>Disconnect</Text>
            </TouchableOpacity>
          </View>
        </View>
      ) : (
        <View style={styles.walletOptions}>
          <TouchableOpacity
            style={styles.walletOption}
            onPress={() => handleConnectWallet("freighter")}
            disabled={connecting}
          >
            <Ionicons name="rocket-outline" size={28} color={COLORS.primary} />
            <View style={styles.walletOptionInfo}>
              <Text style={styles.walletOptionTitle}>Freighter</Text>
              <Text style={styles.walletOptionDesc}>
                Connect with Freighter browser extension
              </Text>
            </View>
            <Ionicons name="chevron-forward" size={20} color="#999" />
          </TouchableOpacity>

          <TouchableOpacity
            style={styles.walletOption}
            onPress={() => handleConnectWallet("albedo")}
            disabled={connecting}
          >
            <Ionicons name="globe-outline" size={28} color={COLORS.primary} />
            <View style={styles.walletOptionInfo}>
              <Text style={styles.walletOptionTitle}>Albedo</Text>
              <Text style={styles.walletOptionDesc}>
                Connect with Albedo wallet
              </Text>
            </View>
            <Ionicons name="chevron-forward" size={20} color="#999" />
          </TouchableOpacity>

          <TouchableOpacity
            style={styles.walletOption}
            onPress={() => handleConnectWallet("local")}
            disabled={connecting}
          >
            <Ionicons name="key-outline" size={28} color={COLORS.primary} />
            <View style={styles.walletOptionInfo}>
              <Text style={styles.walletOptionTitle}>Local Key</Text>
              <Text style={styles.walletOptionDesc}>
                Use a locally stored Stellar keypair
              </Text>
            </View>
            <Ionicons name="chevron-forward" size={20} color="#999" />
          </TouchableOpacity>

          {connecting && (
            <View style={styles.connectingContainer}>
              <Text style={styles.connectingText}>Connecting...</Text>
            </View>
          )}
        </View>
      )}
    </View>
  );

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      {step < 3 && step !== 4 && (
        <View style={styles.header}>
          <TouchableOpacity onPress={handleBack} style={styles.backButton}>
            <Ionicons name="arrow-back" size={24} color={COLORS.black} />
          </TouchableOpacity>
          <Text style={styles.headerTitle}>
            {step === 2 ? "Summary & confirmation" : "Social Transfer"}
          </Text>
          <View style={{ width: 40 }} />
        </View>
      )}

      {step === 4 && (
        <View style={styles.header}>
          <TouchableOpacity onPress={handleBack} style={styles.backButton}>
            <Ionicons name="arrow-back" size={24} color={COLORS.black} />
          </TouchableOpacity>
          <Text style={styles.headerTitle}>Connect Wallet</Text>
          <View style={{ width: 40 }} />
        </View>
      )}

      <ScrollView
        contentContainerStyle={[
          styles.scrollContent,
          (step === 3 || step === 4) && { justifyContent: "center" },
        ]}
        showsVerticalScrollIndicator={false}
      >
        {step === 0 && renderStep0()}
        {step === 1 && renderStep1()}
        {step === 2 && renderStep2()}
        {step === 3 && renderStep3()}
        {step === 4 && renderStep4()}
      </ScrollView>

      {step !== 4 && (
        <View style={styles.footer}>
          <Button
            title={
              step === 1
                ? "Review"
                : step === 2
                  ? submitting
                    ? "Submitting..."
                    : "Confirm & Pay"
                  : step === 3
                    ? "Done"
                    : "Continue"
            }
            onPress={handleNext}
            loading={submitting}
            disabled={
              (step === 0 && !transferType) ||
              (step === 1 && (!recipient || !amount)) ||
              (step === 2 && false) ||
              (step === 3 && false) ||
              submitting
            }
            style={{ backgroundColor: "#1A4B4A" }}
          />
        </View>
      )}
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
    flexGrow: 1,
  },
  stepContainer: {
    flex: 1,
  },
  centerContent: {
    justifyContent: "center",
    alignItems: "center",
  },
  subtitle: {
    fontSize: 16,
    color: "#666",
    marginBottom: 24,
    fontFamily: "Outfit_500Medium",
  },
  cardsContainer: {
    gap: 16,
    marginBottom: 32,
  },
  inputsSection: {
    marginBottom: 16,
    gap: 12,
  },
  transferInput: {
    borderWidth: 1,
    borderColor: COLORS.gray,
    height: 60,
  },
  amountDisplayContainer: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 16,
    gap: 8,
  },
  nairaSymbol: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  amountText: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  keypadContainer: {
    marginTop: 16,
    gap: 12,
  },
  keypadRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    gap: 12,
  },
  keypadButton: {
    flex: 1,
    height: 60,
    backgroundColor: "#F5F5F5",
    borderRadius: 12,
    justifyContent: "center",
    alignItems: "center",
  },
  keypadButtonText: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  visibilitySection: {
    marginBottom: 24,
  },
  sectionLabel: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginBottom: 10,
  },
  visibilityOptions: {
    flexDirection: "row",
    gap: 8,
  },
  visibilityBtn: {
    flex: 1,
    flexDirection: "row",
    height: 44,
    borderWidth: 1,
    borderColor: "#E0E0E0",
    borderRadius: 22,
    justifyContent: "center",
    alignItems: "center",
    gap: 6,
    backgroundColor: "#FDFDFD",
  },
  visibilityBtnActive: {
    backgroundColor: COLORS.primary,
    borderColor: COLORS.primary,
  },
  visibilityText: {
    fontSize: 13,
    fontFamily: "Outfit_500Medium",
    color: "#555",
  },
  visibilityTextActive: {
    color: COLORS.secondary,
    fontFamily: "Outfit_700Bold",
  },
  visibilityDesc: {
    fontSize: 12,
    color: "#777",
    marginTop: 8,
    fontFamily: "Outfit_400Regular",
  },
  payWithSection: {
    flex: 1,
    marginTop: 12,
  },
  payWithLabel: {
    fontSize: 18,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginBottom: 16,
  },
  tokenList: {
    gap: 12,
  },
  tokenCard: {
    flexDirection: "row",
    alignItems: "center",
    padding: 16,
    borderRadius: 100,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    backgroundColor: COLORS.white,
  },
  tokenCardSelected: {
    borderColor: COLORS.primary,
    backgroundColor: "#F0FDF4",
  },
  tokenIcon: {
    width: 48,
    height: 48,
    borderRadius: 24,
    backgroundColor: "#F5F5F5",
    justifyContent: "center",
    alignItems: "center",
    marginRight: 12,
  },
  tokenInfo: {
    flex: 1,
  },
  tokenSymbol: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  tokenBalance: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#666",
  },
  tokenValue: {
    fontSize: 16,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
  },
  summaryCardLarge: {
    backgroundColor: COLORS.white,
    borderRadius: 24,
    padding: 24,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    alignItems: "center",
    marginTop: 10,
  },
  summaryIconLarge: {
    width: 80,
    height: 80,
    borderRadius: 40,
    backgroundColor: "#F5F5F5",
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 16,
  },
  summaryAmountText: {
    fontSize: 26,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  summaryFiatText: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
    color: "#666",
    marginTop: 4,
  },
  divider: {
    height: 1,
    backgroundColor: "#F0F0F0",
    width: "100%",
    marginVertical: 20,
  },
  infoRow: {
    flexDirection: "row",
    alignItems: "center",
    width: "100%",
  },
  recipientBadge: {
    width: 36,
    height: 36,
    borderRadius: 18,
    backgroundColor: "#F5F5F5",
    justifyContent: "center",
    alignItems: "center",
    marginRight: 12,
  },
  infoCol: {
    flex: 1,
  },
  infoLabel: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  infoValue: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginTop: 2,
  },
  successOuter: {
    width: 250,
    height: 250,
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 40,
  },
  successRing: {
    position: "absolute",
    borderRadius: 999,
    borderWidth: 2,
    borderColor: "#EFEFEF",
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
  successTitle: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 20,
  },
  amountCapsule: {
    borderWidth: 1.5,
    borderColor: COLORS.black,
    borderRadius: 100,
    paddingHorizontal: 24,
    paddingVertical: 12,
  },
  amountCapsuleText: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  connectedCard: {
    alignItems: "center",
    padding: 32,
    backgroundColor: "#FAFAFA",
    borderRadius: 24,
    borderWidth: 1,
    borderColor: "#E0E0E0",
    marginTop: 20,
  },
  connectedIcon: {
    width: 64,
    height: 64,
    borderRadius: 32,
    backgroundColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 16,
  },
  connectedTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 8,
  },
  connectedAddress: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
    color: "#666",
    marginBottom: 4,
  },
  connectedSource: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    marginBottom: 8,
  },
  connectedActions: {
    width: "100%",
    marginTop: 8,
  },
  disconnectBtn: {
    marginTop: 12,
    alignItems: "center",
  },
  disconnectText: {
    fontSize: 14,
    fontFamily: "Outfit_500Medium",
    color: "#CC0000",
  },
  walletOptions: {
    gap: 12,
    marginTop: 20,
  },
  walletOption: {
    flexDirection: "row",
    alignItems: "center",
    padding: 16,
    backgroundColor: "#FAFAFA",
    borderRadius: 16,
    borderWidth: 1,
    borderColor: "#E8E8E8",
  },
  walletOptionInfo: {
    flex: 1,
    marginLeft: 12,
  },
  walletOptionTitle: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
  },
  walletOptionDesc: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#777",
    marginTop: 2,
  },
  connectingContainer: {
    alignItems: "center",
    padding: 16,
  },
  connectingText: {
    fontSize: 14,
    fontFamily: "Outfit_500Medium",
    color: "#666",
  },
  footer: {
    padding: 20,
    paddingBottom: Platform.OS === "ios" ? 40 : 20,
  },

  // ── Recipient search dropdown ──────────────────────────────────────────────
  recipientSearchWrapper: {
    position: "relative",
    zIndex: 10,
  },
  searchingIndicator: {
    position: "absolute",
    right: 16,
    top: 20,
  },
  dropdownContainer: {
    position: "absolute",
    top: 64,
    left: 0,
    right: 0,
    backgroundColor: COLORS.white,
    borderWidth: 1,
    borderColor: "#E8E8E8",
    borderRadius: 16,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.08,
    shadowRadius: 12,
    elevation: 6,
    overflow: "hidden",
    zIndex: 20,
  },
  dropdownItem: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 14,
    paddingVertical: 12,
    borderBottomWidth: 1,
    borderBottomColor: "#F5F5F5",
    gap: 12,
  },
  dropdownAvatar: {
    width: 38,
    height: 38,
    borderRadius: 19,
    backgroundColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
  },
  dropdownAvatarText: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.secondary,
  },
  dropdownInfo: {
    flex: 1,
  },
  dropdownUsername: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
  },
  dropdownAddress: {
    fontSize: 11,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    marginTop: 2,
  },
});

export default function TransferScreenWithBoundary() {
  return (
    <ErrorBoundary>
      <TransferScreen />
    </ErrorBoundary>
  );
}
