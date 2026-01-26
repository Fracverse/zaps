import React from "react";
import { View, Text, StyleSheet, TouchableOpacity } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Ionicons } from "@expo/vector-icons";

import WalletIcon from "../assets/wallet.svg";

export default function CreateWalletScreen() {
  const router = useRouter();

  const handleContinue = () => {
    router.push("/backup-key");
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
        <Text style={styles.headerTitle}>Create Your Wallet</Text>
        <View style={{ width: 24 }} />
      </View>

      <View style={styles.content}>
        <View style={styles.imageContainer}>
          <WalletIcon width={100} height={100} />
        </View>

        <View style={styles.textContainer}>
          <Text style={styles.title}>Non-Custodial Wallet</Text>
          <Text style={styles.subtitle}>
            Your wallet is secured by a secret key that only you control. No one
            else can access your funds.
          </Text>
        </View>
      </View>

      <View style={styles.footer}>
        <Button title="Continue" onPress={handleContinue} variant="primary" />
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
    flex: 1,
    paddingHorizontal: 20,
    justifyContent: "center",
    alignItems: "center",
    paddingBottom: 80,
  },
  imageContainer: {
    marginBottom: 40,
    width: 160,
    height: 160,
    justifyContent: "center",
    alignItems: "center",
    borderRadius: 100, // Circular border
    borderWidth: 1,
    borderColor: "#EFEFEF", // Light grey border
  },
  textContainer: {
    alignItems: "center",
    paddingHorizontal: 20,
  },
  title: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 12,
    textAlign: "center",
  },
  subtitle: {
    fontSize: 16,
    color: "#666",
    textAlign: "center",
    lineHeight: 24,
    fontFamily: "Outfit_400Regular",
  },
  footer: {
    padding: 20,
    paddingBottom: 40,
  },
});
