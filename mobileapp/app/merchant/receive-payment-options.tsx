import React, { useState } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import { COLORS } from "../../src/constants/colors";
import { Button } from "../../src/components/Button";
import { AccountTypeCard } from "../../src/components/AccountTypeCard";
import { Ionicons } from "@expo/vector-icons";

import TapIconBlack from "../../assets/icon-3-black.svg";
import ScanIconBlack from "../../assets/QrCode-black.svg";

export default function ReceivePaymentOptionsScreen() {
  const router = useRouter();
  const [selectedMethod, setSelectedMethod] = useState<"tap" | "qr" | null>(
    null
  );

  const handleContinue = () => {
    if (selectedMethod === "tap") {
      router.push("/merchant/accept-payment");
    } else if (selectedMethod === "qr") {
      router.push("/merchant/qr-code");
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.header}>
        <TouchableOpacity
          style={styles.backButton}
          onPress={() => router.back()}
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Receive Payment</Text>
        <View style={{ width: 24 }} />
      </View>

      <ScrollView contentContainerStyle={styles.content}>
        <View style={styles.cardsContainer}>
          <AccountTypeCard
            title="Receive Via Tap"
            description="Receive crypto via Near Field Communication from Blink users"
            Icon={TapIconBlack}
            selected={selectedMethod === "tap"}
            onPress={() => setSelectedMethod("tap")}
          />

          <AccountTypeCard
            title="Receive Via QR Code"
            description="Receive crypto via Quick Response code from Blink users"
            Icon={ScanIconBlack}
            selected={selectedMethod === "qr"}
            onPress={() => setSelectedMethod("qr")}
          />
        </View>
      </ScrollView>

      <View style={styles.footer}>
        <Button
          title="Review"
          onPress={handleContinue}
          variant="primary"
          style={selectedMethod ? styles.activeButton : styles.disabledButton}
          textStyle={styles.buttonText}
          loading={false}
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
  content: {
    paddingHorizontal: 20,
    paddingTop: 30,
  },
  cardsContainer: {
    gap: 16,
  },
  footer: {
    padding: 20,
    paddingBottom: 30,
    marginTop: "auto",
  },
  activeButton: {
    backgroundColor: "#1A4B4A", // Dark green from screenshot
  },
  disabledButton: {
    backgroundColor: "#1A4B4A",
    opacity: 0.5,
  },
  buttonText: {
    color: COLORS.white,
  },
});
