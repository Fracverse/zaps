import React, { useState, useEffect, useRef } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  LayoutAnimation,
  Platform,
  UIManager,
  Animated,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter, Stack } from "expo-router";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Input } from "../src/components/Input";
import { NfcIcon } from "../src/components/NfcIcon";

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
    balance: "100.00",
    value: "125.32",
    Icon: XLMLogo,
  },
  {
    id: "usdt",
    symbol: "USDT",
    balance: "100.00",
    value: "100",
    Icon: USDTLogo,
  },
  {
    id: "usdc",
    symbol: "USDC",
    balance: "100.00",
    value: "100",
    Icon: USDCLogo,
  },
];

const MERCHANT_NAME = "Ebube One";
const CONFIRM_AMOUNT = "$100";

type Step =
  | "ready"
  | "searching"
  | "terminalFound"
  | "transfer"
  | "confirm"
  | "success";

function PulsingCircles() {
  const anim1 = useRef(new Animated.Value(0)).current;
  const anim2 = useRef(new Animated.Value(0)).current;
  const anim3 = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    const duration = 1800;
    const createPulse = (anim: Animated.Value, delay: number) =>
      Animated.loop(
        Animated.sequence([
          Animated.delay(delay),
          Animated.timing(anim, {
            toValue: 1,
            duration,
            useNativeDriver: true,
          }),
          Animated.timing(anim, {
            toValue: 0,
            duration: 0,
            useNativeDriver: true,
          }),
        ])
      );

    const a1 = createPulse(anim1, 0);
    const a2 = createPulse(anim2, 400);
    const a3 = createPulse(anim3, 800);
    a1.start();
    a2.start();
    a3.start();
    return () => {
      a1.stop();
      a2.stop();
      a3.stop();
    };
  }, [anim1, anim2, anim3]);

  const scale1 = anim1.interpolate({
    inputRange: [0, 1],
    outputRange: [0.5, 1.2],
  });
  const o1 = anim1.interpolate({
    inputRange: [0, 0.5, 1],
    outputRange: [0.3, 0.15, 0],
  });
  const scale2 = anim2.interpolate({
    inputRange: [0, 1],
    outputRange: [0.5, 1.4],
  });
  const o2 = anim2.interpolate({
    inputRange: [0, 0.5, 1],
    outputRange: [0.25, 0.12, 0],
  });
  const scale3 = anim3.interpolate({
    inputRange: [0, 1],
    outputRange: [0.5, 1.6],
  });
  const o3 = anim3.interpolate({
    inputRange: [0, 0.5, 1],
    outputRange: [0.2, 0.1, 0],
  });

  const ringSize = 100;
  return (
    <View style={pulseStyles.container}>
      <Animated.View
        style={[
          pulseStyles.ring,
          {
            width: ringSize,
            height: ringSize,
            borderRadius: ringSize / 2,
            opacity: o1,
            transform: [{ scale: scale1 }],
          },
        ]}
      />
      <Animated.View
        style={[
          pulseStyles.ring,
          {
            width: ringSize,
            height: ringSize,
            borderRadius: ringSize / 2,
            opacity: o2,
            transform: [{ scale: scale2 }],
          },
        ]}
      />
      <Animated.View
        style={[
          pulseStyles.ring,
          {
            width: ringSize,
            height: ringSize,
            borderRadius: ringSize / 2,
            opacity: o3,
            transform: [{ scale: scale3 }],
          },
        ]}
      />
      <View style={pulseStyles.iconWrap}>
        <NfcIcon size={56} color={COLORS.primary} fillCircle />
      </View>
    </View>
  );
}

const pulseStyles = StyleSheet.create({
  container: {
    width: 200,
    height: 200,
    justifyContent: "center",
    alignItems: "center",
  },
  ring: {
    position: "absolute",
    borderWidth: 2,
    borderColor: "#E0E0E0",
    borderStyle: "dashed",
  },
  iconWrap: {
    justifyContent: "center",
    alignItems: "center",
  },
});

const TokenSelectCard = ({
  symbol,
  balance,
  value,
  Icon,
  selected,
  onPress,
}: {
  symbol: string;
  balance: string;
  value: string;
  Icon: React.FC<any>;
  selected: boolean;
  onPress: () => void;
}) => (
  <TouchableOpacity
    style={[styles.tokenCard, selected && styles.tokenCardSelected]}
    onPress={onPress}
    activeOpacity={0.8}
    accessibilityRole="radio"
    accessibilityState={{ selected }}
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

export default function TapToPayScreen() {
  const router = useRouter();
  const [step, setStep] = useState<Step>("ready");
  const [amount, setAmount] = useState("");
  const [selectedToken, setSelectedToken] = useState(TOKENS[0].id);

  useEffect(() => {
    if (step === "terminalFound") {
      const t = setTimeout(() => {
        LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
        setStep("transfer");
      }, 1500);
      return () => clearTimeout(t);
    }
  }, [step]);

  const goNext = () => {
    LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
    if (step === "ready") setStep("searching");
    else if (step === "searching") setStep("terminalFound");
    else if (step === "transfer") setStep("confirm");
    else if (step === "confirm") setStep("success");
    else if (step === "success") router.replace("/(personal)/home");
  };

  const handleBack = () => {
    if (step === "ready") router.back();
    else if (step === "success") router.replace("/(personal)/home");
    else {
      LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
      if (step === "transfer" || step === "confirm") setStep("terminalFound");
      else if (step === "terminalFound") setStep("searching");
      else setStep("ready");
    }
  };

  const showHeader = true;
  const showBack = step !== "success";
  const headerTitle =
    step === "success"
      ? "Success"
      : step === "confirm"
        ? "Confirm Payment"
        : step === "transfer"
          ? "Transfer"
          : "Tap to Pay";

  const renderContent = () => {
    if (step === "ready") {
      return (
        <View style={styles.centeredBlock}>
          <View style={styles.nfcIconCircle}>
            <NfcIcon size={72} color={COLORS.primary} fillCircle />
          </View>
          <Text style={styles.readyTitle}>Ready to Pay</Text>
          <Text style={styles.readySubtext}>
            Hold your phone near the payment terminal.
          </Text>
        </View>
      );
    }

    if (step === "searching" || step === "terminalFound") {
      return (
        <View style={styles.centeredBlock}>
          <PulsingCircles />
          <Text style={styles.statusTitle}>
            {step === "searching" ? "Searching..." : "Terminal Found"}
          </Text>
          <Text style={styles.statusSubtext}>
            {step === "searching"
              ? "Looking for nearby terminal"
              : "Connecting..."}
          </Text>
        </View>
      );
    }

    if (step === "transfer" || step === "confirm") {
      return (
        <ScrollView
          style={styles.scroll}
          contentContainerStyle={styles.scrollContent}
          showsVerticalScrollIndicator={false}
          keyboardShouldPersistTaps="handled"
        >
          <View style={styles.merchantSection}>
            <View style={styles.nfcIconCircleSmall}>
              <NfcIcon size={48} color={COLORS.primary} fillCircle />
            </View>
            {step === "confirm" && (
              <Text style={styles.amountLarge}>{CONFIRM_AMOUNT}</Text>
            )}
            <View style={styles.merchantRow}>
              <View style={styles.merchantIcon}>
                <Ionicons name="storefront-outline" size={20} color="#666" />
              </View>
              <View>
                <Text style={styles.merchantLabel}>Merchant</Text>
                <Text style={styles.merchantName}>{MERCHANT_NAME}</Text>
              </View>
            </View>
          </View>

          {step === "transfer" && (
            <View style={styles.inputSection}>
              <Input
                placeholder="Amount"
                value={amount}
                onChangeText={setAmount}
                keyboardType="decimal-pad"
                style={styles.amountInput}
              />
            </View>
          )}

          <View style={styles.payWithSection}>
            <Text style={styles.payWithLabel}>Pay with</Text>
            <View style={styles.tokenList}>
              {TOKENS.map((t) => (
                <TokenSelectCard
                  key={t.id}
                  symbol={t.symbol}
                  balance={t.balance}
                  value={t.value}
                  Icon={t.Icon}
                  selected={selectedToken === t.id}
                  onPress={() => setSelectedToken(t.id)}
                />
              ))}
            </View>
          </View>
        </ScrollView>
      );
    }

    if (step === "success") {
      return (
        <View style={styles.centeredBlock}>
          <View style={styles.successOuter}>
            <View
              style={[
                styles.successRing,
                { width: 180, height: 180, opacity: 0.35 },
              ]}
            />
            <View
              style={[
                styles.successRing,
                { width: 140, height: 140, opacity: 0.4 },
              ]}
            />
            <View style={styles.successCheck}>
              <Ionicons name="checkmark" size={56} color={COLORS.primary} />
            </View>
          </View>
          <Text style={styles.successTitle}>Transfer Successful</Text>
          <View style={styles.amountCapsule}>
            <Text style={styles.amountCapsuleText}>{CONFIRM_AMOUNT}</Text>
          </View>
        </View>
      );
    }

    return null;
  };

  const footerButton = () => {
    if (step === "ready") {
      return (
        <Button
          title="Continue"
          onPress={goNext}
          style={styles.primaryButton}
        />
      );
    }
    if (step === "searching" || step === "terminalFound") {
      return (
        <Button
          title="Scan instead"
          onPress={() => router.replace("/scan")}
          style={styles.primaryButton}
          textStyle={styles.scanInsteadText}
        />
      );
    }
    if (step === "transfer" || step === "confirm") {
      return (
        <Button
          title="Transfer"
          onPress={goNext}
          icon={
            <Ionicons
              name="refresh-outline"
              size={20}
              color={COLORS.secondary}
              style={{ marginRight: 8, transform: [{ rotate: "45deg" }] }}
            />
          }
          disabled={step === "transfer" && !amount}
          style={styles.primaryButton}
        />
      );
    }
    if (step === "success") {
      return (
        <Button title="Done" onPress={goNext} style={styles.primaryButton} />
      );
    }
    return null;
  };

  return (
    <SafeAreaView style={styles.container} edges={["top"]}>
      <Stack.Screen options={{ headerShown: false }} />

      {showHeader && (
        <View style={styles.header}>
          {showBack ? (
            <TouchableOpacity
              onPress={handleBack}
              style={styles.backButton}
              accessibilityLabel="Go back"
              accessibilityRole="button"
            >
              <Ionicons name="arrow-back" size={24} color={COLORS.black} />
            </TouchableOpacity>
          ) : (
            <View style={styles.backButton} />
          )}
          <Text
            style={[
              styles.headerTitle,
              step === "success" && styles.headerTitleSuccess,
            ]}
          >
            {headerTitle}
          </Text>
          <View style={styles.headerSpacer} />
        </View>
      )}

      <View
        style={[
          styles.content,
          (step === "success" ||
            step === "ready" ||
            step === "searching" ||
            step === "terminalFound") &&
            styles.contentCentered,
        ]}
      >
        {renderContent()}
      </View>

      <View style={styles.footer}>{footerButton()}</View>
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
  headerTitleSuccess: {
    color: COLORS.primary,
  },
  headerSpacer: {
    width: 40,
  },
  content: {
    flex: 1,
  },
  contentCentered: {
    justifyContent: "center",
    alignItems: "center",
  },
  centeredBlock: {
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 24,
  },
  nfcIconCircle: {
    marginBottom: 24,
  },
  nfcIconCircleSmall: {
    marginBottom: 12,
  },
  readyTitle: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 8,
    textAlign: "center",
  },
  readySubtext: {
    fontSize: 16,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    textAlign: "center",
  },
  statusTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginTop: 24,
    marginBottom: 4,
  },
  statusSubtext: {
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  scroll: {
    flex: 1,
  },
  scrollContent: {
    paddingHorizontal: 20,
    paddingTop: 8,
    paddingBottom: 24,
    maxWidth: 500,
    width: "100%",
    alignSelf: "center",
  },
  merchantSection: {
    alignItems: "center",
    marginBottom: 20,
  },
  amountLarge: {
    fontSize: 32,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 12,
  },
  merchantRow: {
    flexDirection: "row",
    alignItems: "center",
  },
  merchantIcon: {
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: "#F5F5F5",
    justifyContent: "center",
    alignItems: "center",
    marginRight: 12,
  },
  merchantLabel: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  merchantName: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginTop: 2,
  },
  inputSection: {
    marginBottom: 20,
  },
  amountInput: {
    borderWidth: 1,
    borderColor: COLORS.gray,
    height: 64,
  },
  payWithSection: {
    marginBottom: 16,
  },
  payWithLabel: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 12,
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
    borderWidth: 1.5,
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
  successOuter: {
    width: 220,
    height: 220,
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 32,
  },
  successRing: {
    position: "absolute",
    borderRadius: 999,
    borderWidth: 2,
    borderColor: "#D0E8E6",
    borderStyle: "dashed",
  },
  successCheck: {
    width: 100,
    height: 100,
    borderRadius: 50,
    borderWidth: 4,
    borderColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
    backgroundColor: COLORS.white,
  },
  successTitle: {
    fontSize: 20,
    fontFamily: "Outfit_600SemiBold",
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
    paddingBottom: Platform.OS === "ios" ? 34 : 20,
  },
  primaryButton: {
    backgroundColor: COLORS.primary,
  },
  scanInsteadText: {
    color: COLORS.secondary,
  },
});
