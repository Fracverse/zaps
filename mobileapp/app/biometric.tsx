import React from "react";
import { View, Text, StyleSheet, TouchableOpacity, Image } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Ionicons } from "@expo/vector-icons";

export default function BiometricScreen() {
  const router = useRouter();

  const handleContinue = () => {
    // Logic to enable biometric would go here
    console.log("Biometric enabled");
    router.push("/create-wallet");
  };

  const handleSkip = () => {
    // Navigate to the personal dashboard
    router.push("/create-wallet");
    // alert("Setup Complete!");
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
        <Text style={styles.headerTitle}>Secure Your Account</Text>
        <View style={{ width: 24 }} />
      </View>

      <View style={styles.content}>
        <View style={styles.iconContainer}>
          <Image
            source={require("../assets/fingerprint.png")}
            style={styles.fingerprint}
            resizeMode="contain"
          />
        </View>

        <View style={styles.textContainer}>
          <Text style={styles.title}>Enable Biometric Login</Text>
          <Text style={styles.subtitle}>
            Use your fingerprint or face to quickly and securely access your
            wallet
          </Text>
        </View>
      </View>

      <View style={styles.footer}>
        <Button
          title="Continue"
          onPress={handleContinue}
          variant="primary"
          style={styles.mainButton}
        />
        <Button
          title="Skip for now"
          onPress={handleSkip}
          variant="secondary"
          style={styles.skipButton}
          textStyle={{ color: COLORS.primary }}
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
    flex: 1,
    paddingHorizontal: 20,
    justifyContent: "center",
    alignItems: "center",
    paddingBottom: 100, // Visual balance
  },
  iconContainer: {
    width: 120,
    height: 120,
    borderRadius: 60,
    backgroundColor: "transparent",
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 40,
    borderWidth: 1,
    borderColor: "#eee",
  },
  fingerprint: {
    width: 60,
    height: 60,
    tintColor: COLORS.primary,
  },
  textContainer: {
    alignItems: "center",
  },
  title: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 12,
  },
  subtitle: {
    fontSize: 16,
    color: "#666",
    textAlign: "center",
    lineHeight: 24,
    maxWidth: "80%",
    fontFamily: "Outfit_500Medium",
  },
  footer: {
    padding: 20,
    paddingBottom: 40,
    gap: 12,
  },
  mainButton: {
    marginBottom: 0,
  },
  skipButton: {
    backgroundColor: COLORS.secondary,
  },
});
