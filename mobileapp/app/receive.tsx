import React, { useState } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  LayoutAnimation,
  Platform,
  UIManager,
  Share,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter, Stack } from "expo-router";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { AccountTypeCard } from "../src/components/AccountTypeCard";

import ZapIcon from "../assets/icon-4.svg";
import WalletIcon from "../assets/wallet.svg";

if (
  Platform.OS === "android" &&
  UIManager.setLayoutAnimationEnabledExperimental
) {
  UIManager.setLayoutAnimationEnabledExperimental(true);
}

export default function ReceiveScreen() {
  const router = useRouter();
  const [step, setStep] = useState(0);
  const [receiveType, setReceiveType] = useState<"zaps" | "external" | null>(
    null
  );

  const blinksId = "ejembiii.blinks";
  const walletAddress = "GABC...1234"; // Placeholder

  const handleNext = () => {
    LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
    setStep(step + 1);
  };

  const handleBack = () => {
    if (step === 0) {
      router.back();
    } else {
      LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
      setStep(step - 1);
    }
  };

  const handleShare = async () => {
    try {
      await Share.share({
        message: receiveType === "zaps" ? blinksId : walletAddress,
      });
    } catch (error) {
      console.log(error);
    }
  };

  const renderStep0 = () => (
    <View style={styles.stepContainer}>
      <Text style={styles.subtitle}>Choose how you want to receive money.</Text>
      <View style={styles.cardsContainer}>
        <AccountTypeCard
          title="Blinx User"
          description="Receive instantly from any Blinx user via your Zaps ID"
          Icon={ZapIcon}
          selected={receiveType === "zaps"}
          onPress={() => setReceiveType("zaps")}
        />
        <AccountTypeCard
          title="External Wallet"
          description="Receive from any XLM or Stellar compatible wallet address"
          Icon={WalletIcon}
          selected={receiveType === "external"}
          onPress={() => setReceiveType("external")}
        />
      </View>
    </View>
  );

  const renderStep1 = () => (
    <View style={styles.stepContainer}>
      <View style={styles.qrContainer}>
        <View style={styles.qrCard}>
          {/* QR Code Placeholder with pattern background */}
          <View style={styles.qrPatternBackground}>
            <Ionicons name="qr-code-outline" size={200} color={COLORS.black} />
          </View>
        </View>

        <View style={styles.idDisplaySection}>
          <View style={styles.idBadge}>
            <ZapIcon width={18} height={18} />
          </View>
          <View style={styles.idTextContainer}>
            <Text style={styles.idLabel}>
              {receiveType === "zaps" ? "Blinks ID" : "Wallet Address"}
            </Text>
            <Text style={styles.idValue}>
              {receiveType === "zaps" ? blinksId : walletAddress}
            </Text>
          </View>
        </View>

        <View style={styles.divider} />

        <View style={styles.actionRowSide}>
          <TouchableOpacity style={styles.outlineActionBtn} activeOpacity={0.7}>
            <Ionicons name="copy-outline" size={20} color="#0E4A47" />
            <Text style={styles.outlineActionText}>Copy ID</Text>
          </TouchableOpacity>
          <TouchableOpacity
            style={styles.outlineActionBtn}
            activeOpacity={0.7}
            onPress={handleShare}
          >
            <Ionicons name="share-outline" size={20} color="#0E4A47" />
            <Text style={styles.outlineActionText}>Share ID</Text>
          </TouchableOpacity>
        </View>
      </View>
    </View>
  );

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.header}>
        <TouchableOpacity onPress={handleBack} style={styles.backButton}>
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Receive</Text>
        <View style={{ width: 40 }} />
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        {step === 0 ? renderStep0() : renderStep1()}
      </ScrollView>

      <View style={styles.footer}>
        {step === 0 && (
          <Button title="Review" onPress={handleNext} disabled={!receiveType} />
        )}
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
    alignItems: "center",
    width: "100%",
  },
  subtitle: {
    fontSize: 16,
    color: "#666",
    marginBottom: 24,
    fontFamily: "Outfit_500Medium",
    alignSelf: "flex-start",
  },
  cardsContainer: {
    gap: 16,
    marginBottom: 32,
    width: "100%",
  },
  qrContainer: {
    alignItems: "center",
    width: "100%",
    marginTop: 10,
  },
  qrCard: {
    width: "100%",
    aspectRatio: 1,
    backgroundColor: "#F5F5F5",
    borderRadius: 24,
    padding: 2,
    overflow: "hidden",
    marginBottom: 24,
  },
  qrPatternBackground: {
    flex: 1,
    backgroundColor: "#EFEFEF",
    justifyContent: "center",
    alignItems: "center",
    borderRadius: 22,
  },
  idDisplaySection: {
    flexDirection: "row",
    alignItems: "center",
    width: "100%",
    paddingHorizontal: 4,
  },
  idBadge: {
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: "#F5F5F5",
    justifyContent: "center",
    alignItems: "center",
    marginRight: 12,
  },
  idTextContainer: {
    flex: 1,
  },
  idLabel: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  idValue: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginTop: 2,
  },
  divider: {
    height: 1,
    backgroundColor: "#F0F0F0",
    width: "100%",
    marginVertical: 24,
  },
  actionRowSide: {
    flexDirection: "row",
    width: "100%",
    gap: 16,
  },
  outlineActionBtn: {
    flex: 1,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    paddingVertical: 16,
    borderRadius: 100,
    borderWidth: 1,
    borderColor: "#0E4A47",
    gap: 8,
  },
  outlineActionText: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: "#0E4A47",
  },
  footer: {
    padding: 20,
    paddingBottom: Platform.OS === "ios" ? 40 : 20,
  },
});
