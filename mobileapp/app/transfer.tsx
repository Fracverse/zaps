import React, { useState } from "react";
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
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter, Stack } from "expo-router";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Input } from "../src/components/Input";
import { AccountTypeCard } from "../src/components/AccountTypeCard";
import AsyncStorage from "@react-native-async-storage/async-storage";

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

  const token = TOKENS.find((t) => t.id === selectedToken) || TOKENS[0];

  const handleNext = async () => {
    if (step === 2) {
      // Save payment state to AsyncStorage so home screen updates
      try {
        const stored = await AsyncStorage.getItem("pending_transfers");
        const list = stored ? JSON.parse(stored) : [];
        list.unshift({
          recipient,
          amount,
          description: description || "Sent money 💸",
          visibility,
          token: token.symbol,
        });
        await AsyncStorage.setItem("pending_transfers", JSON.stringify(list));
      } catch (e) {
        console.error(e);
      }
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
    } else {
      LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
      setStep(step - 1);
    }
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
        <Input
          placeholder={
            transferType === "ZAPS"
              ? "Recipient ZAPS ID (e.g. tolu.zaps)"
              : "Wallet Address"
          }
          value={recipient}
          onChangeText={setRecipient}
          autoCapitalize="none"
          style={styles.transferInput}
        />

        {/* Custom Amount Display */}
        <TouchableOpacity activeOpacity={1} style={[styles.transferInput, styles.amountDisplayContainer]}>
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
            <TouchableOpacity style={styles.keypadButton} onPress={() => setAmount((prev: string) => prev + ".")}>
              <Text style={styles.keypadButtonText}>.</Text>
            </TouchableOpacity>
            <TouchableOpacity style={styles.keypadButton} onPress={() => setAmount((prev: string) => prev + "0")}>
              <Text style={styles.keypadButtonText}>0</Text>
            </TouchableOpacity>
            <TouchableOpacity style={styles.keypadButton} onPress={() => setAmount((prev: string) => prev.slice(0, -1))}>
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

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      {step < 3 && (
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

      <ScrollView
        contentContainerStyle={[
          styles.scrollContent,
          step === 3 && { justifyContent: "center" },
        ]}
        showsVerticalScrollIndicator={false}
      >
        {step === 0 && renderStep0()}
        {step === 1 && renderStep1()}
        {step === 2 && renderStep2()}
        {step === 3 && renderStep3()}
      </ScrollView>

      <View style={styles.footer}>
        <Button
          title={
            step === 1
              ? "Review"
              : step === 2
                ? "Confirm & Pay"
                : step === 3
                  ? "Done"
                  : "Continue"
          }
          onPress={handleNext}
          disabled={
            (step === 0 && !transferType) ||
            (step === 1 && (!recipient || !amount)) ||
            (step === 2 && false)
          }
          style={{ backgroundColor: "#1A4B4A" }}
        />
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
  footer: {
    padding: 20,
    paddingBottom: Platform.OS === "ios" ? 40 : 20,
  },
});

export default function TransferScreenWithBoundary() {
  return (
    <ErrorBoundary>
      <TransferScreen />
    </ErrorBoundary>
  );
}
