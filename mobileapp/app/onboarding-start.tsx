import React from "react";
import { View, Text, StyleSheet } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import BlinkLogo from "../assets/blinkLogo.svg";

export default function AccountTypeScreen() {
  const router = useRouter();

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.content}>
        <View style={styles.logoContainer}>
            {/* Using logo.png as per existing patterns, assuming it's the correct logo asset */}
             <BlinkLogo width={216} height={103} style={styles.splashLogo} />
            <Text style={styles.splashText}>BLINK</Text>
        </View>
        
        <View style={styles.buttonContainer}>
            <Button
                title="New Account"
                onPress={() => router.push("/username")}
                variant="primary"
                style={styles.button}
            />
            <View style={styles.spacer} />
            <Button
                title="Returning User"
                onPress={() => router.push("/returning-user")}
                variant="secondary" // Using secondary/outline for specific visual hierarchy if available, otherwise primary
                style={styles.buttonTwo}
            />
        </View>
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.secondary, // Green background based on context/defaults
  },
  content: {
    flex: 1,
    justifyContent: "space-between", // Logo top/center, buttons bottom
    paddingVertical: 60,
    paddingHorizontal: 20,
    alignItems: "center",
  },
  logoContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
  },
    splashText: {
    fontSize: 80, // Increased to 80px
    fontFamily: "Anton_400Regular",
    color: COLORS.primary,
    letterSpacing: 4,
    textTransform: "uppercase",
  },
  logo: {
    width: 80,
    height: 80,
    marginBottom: 10,
    tintColor: COLORS.primary,
  },
  brandText: {
    fontSize: 40,
    fontFamily: "Anton_400Regular", // Using Anton as seen in index.tsx
    color: COLORS.primary,
    textTransform: "uppercase",
    letterSpacing: 2,
  },
  buttonContainer: {
    width: "100%",
    paddingBottom: 20,
  },
  button: {
    height: 60,
    borderRadius: 30,
    backgroundColor: COLORS.primary,
  },
  buttonTwo: {
    height: 60,
    borderRadius: 30,
    backgroundColor: "#6FD784",
  },
    splashLogo: {
    // width/height handled by SVG props
    marginBottom: 0, // Reset margin since using gap
  },
  spacer: {
    height: 16,
  },
});
