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
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { AccountTypeCard } from "../src/components/AccountTypeCard";
import { Ionicons } from "@expo/vector-icons";

import PersonalIcon from "../assets/peronal.svg";
import MerchantIcon from "../assets/merchant.svg";

export default function AccountTypeScreen() {
  const router = useRouter();
  const [selectedType, setSelectedType] = useState<
    "personal" | "merchant" | null
  >(null);

  const handleContinue = () => {
    if (selectedType === "merchant") {
      router.push("/merchant");
    } else if (selectedType === "personal") {
      router.push("/username");
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
        <Text style={styles.headerTitle}>Choose Account Type</Text>
        <View style={{ width: 24 }} />
      </View>

      <ScrollView contentContainerStyle={styles.content}>
        <Text style={styles.subtitle}>
          Select how you'll primarily use Zap.
        </Text>

        <View style={styles.cardsContainer}>
          <AccountTypeCard
            title="Personal"
            description="Send money, pay merchants, and manage your crypto wallet"
            Icon={PersonalIcon}
            selected={selectedType === "personal"}
            onPress={() => setSelectedType("personal")}
          />

          <AccountTypeCard
            title="Merchants"
            description="Accepts crypto payments and recieve USD to your bank"
            Icon={MerchantIcon}
            selected={selectedType === "merchant"}
            onPress={() => setSelectedType("merchant")}
          />
        </View>
      </ScrollView>

      <View style={styles.footer}>
        <Button
          title="Continue"
          onPress={handleContinue}
          variant="primary"
          style={selectedType ? {} : styles.disabledButton}
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
    paddingTop: 20,
  },
  subtitle: {
    fontSize: 16,
    color: "#666",
    marginBottom: 24,
    fontFamily: "Outfit_500Medium",
  },
  cardsContainer: {
    gap: 16,
  },
  footer: {
    padding: 20,
    paddingBottom: 30,
  },
  disabledButton: {
    opacity: 0.5,
  },
});
