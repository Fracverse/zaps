import React, { useState } from "react";
import { View, Text, StyleSheet, TouchableOpacity } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import { COLORS } from "../../src/constants/colors";
import { Button } from "../../src/components/Button";
import { Input } from "../../src/components/Input";
import { Ionicons } from "@expo/vector-icons";

export default function PasswordSetupScreen() {
  const router = useRouter();
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");

  const handleContinue = () => {
    if (password && password === confirmPassword) {
      // Proceed to biometric setup
      router.push("/returning-user/biometric");
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
        <Text style={styles.headerTitle}>Enter your passcode</Text>
        <View style={{ width: 40 }} />
      </View>

      <View style={styles.content}>
        <Text style={styles.description}>
          Set up your personal account to get started.
        </Text>

        <View style={styles.inputContainer}>
          <Input
            placeholder="Enter a strong password"
            value={password}
            onChangeText={setPassword}
            secureTextEntry
          />
          <View style={{height: 16}} />
          <Input
            placeholder="Confirm password"
            value={confirmPassword}
            onChangeText={setConfirmPassword}
            secureTextEntry
          />
        </View>
      </View>

      <View style={styles.footer}>
        <Button
          title="Continue"
          onPress={handleContinue}
          variant="primary"
          style={styles.button}
          disabled={!password || password !== confirmPassword}
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
    paddingTop: 20,
  },
  description: {
    fontSize: 16,
    color: "#666",
    marginBottom: 32,
    fontFamily: "Outfit_400Regular",
  },
  inputContainer: {
    width: "100%",
  },
  footer: {
    padding: 20,
    paddingBottom: 40,
  },
  button: {
    backgroundColor: "#1A4B4A",
    borderRadius: 100,
    height: 60,
  },
});
