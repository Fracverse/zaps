import React from "react";
import { View, Text, StyleSheet, Image, Pressable } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Stack, useRouter } from "expo-router";

import Icon1 from "../assets/icon-1.svg";
import Icon2 from "../assets/icon-2.svg";
import Icon3 from "../assets/icon-3.svg";

export default function OnboardingScreen() {
  const router = useRouter();

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.content}>
        <View style={styles.header}>
          <Image
            source={require("../assets/logo.png")}
            style={styles.logo}
            resizeMode="contain"
          />
        </View>

        <View style={styles.featureContainer}>
          {/* Top Row - Instant */}
          <View style={styles.trackRow}>
            <View style={[styles.featureCard, styles.cardLeft]}>
              <Icon1 style={styles.icon} />
              <Text style={styles.featureText}>Instant</Text>
            </View>
          </View>

          {/* Middle Row - Non-Custodial */}
          <View style={styles.trackRow}>
            <View style={[styles.featureCard, styles.cardCenter]}>
              <Icon2 style={styles.icon} />
              <Text style={styles.featureText}>Non-Custodial</Text>
            </View>
          </View>

          {/* Bottom Row - Tap or Scan */}
          <View style={styles.trackRow}>
            <View style={[styles.featureCard, styles.cardRight]}>
              <Icon3 style={styles.icon} />
              <Text style={styles.featureText}>Tap or Scan</Text>
            </View>
          </View>
        </View>

        <View style={styles.titleContainer}>
          <Text style={styles.title}>PAY OR GET PAID</Text>
          <Text style={styles.title}>WITH CRYPTO</Text>
          <Text style={styles.subtitle}>
            Blink is the fastest to move{"\n"}crypto around
          </Text>
        </View>

        <View style={styles.footer}>
          <Button
            title="Continue"
            onPress={() => router.push("/account-type")}
            variant="primary"
            style={styles.continueButton}
            textStyle={styles.buttonText}
          />
          
          {/* Test Navigation Buttons */}
          <View style={styles.testButtons}>
            <Text style={styles.testLabel}>Test Screens:</Text>
            <Pressable
              style={styles.testButton}
              onPress={() => router.push("/merchant/withdraw-bank")}
            >
              <Text style={styles.testButtonText}>Withdraw</Text>
            </Pressable>
            <Pressable
              style={styles.testButton}
              onPress={() => router.push("/merchant/transfer-summary")}
            >
              <Text style={styles.testButtonText}>Transfer</Text>
            </Pressable>
            <Pressable
              style={styles.testButton}
              onPress={() => router.push("/merchant/success")}
            >
              <Text style={styles.testButtonText}>Success</Text>
            </Pressable>
          </View>
        </View>
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.secondary,
  },
  content: {
    flex: 1,
    paddingHorizontal: 20,
    justifyContent: "space-between",
    paddingVertical: 20,
  },
  header: {
    alignItems: "center",
    marginBottom: 20,
    paddingTop: 20,
  },
  logo: {
    width: 40,
    height: 40,
    tintColor: COLORS.primary,
  },
  featureContainer: {
    flex: 1,
    justifyContent: "center",
    gap: 16, // Space between rows
    paddingHorizontal: 10,
  },
  trackRow: {
    width: "100%",
    backgroundColor: "#74D189", // Slightly darker green for track
    borderRadius: 100, // Full pill shape
    height: 80,
    justifyContent: "center",
    padding: 5,
  },
  featureCard: {
    backgroundColor: COLORS.primary,
    borderRadius: 100, // Full pill shape
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 24,
    paddingVertical: 18,
    position: "absolute",
    height: "100%", // Match track height
  },
  cardLeft: {
    left: 5,
    paddingRight: 40, // Visual balance
    minWidth: "55%",
  },
  cardCenter: {
    alignSelf: "center",
    justifyContent: "center",
    minWidth: "60%",
  },
  cardRight: {
    right: 5,
    paddingLeft: 40,
    minWidth: "55%",
    flexDirection: "row",
    justifyContent: "center",
  },
  icon: {
    marginRight: 10,
    tintColor: "#AEDCBA", // Light green tint for icons inside dark card
  },
  featureText: {
    color: "#80FA98", // Light green text
    fontSize: 18,
    fontFamily: "Outfit_500Medium",
  },
  titleContainer: {
    alignItems: "center",
    marginBottom: 30,
  },
  title: {
    fontSize: 42, // Larger as per screenshot
    fontFamily: "Anton_400Regular",
    color: COLORS.primary,
    textAlign: "center",
    lineHeight: 50,
    textTransform: "uppercase",
  },
  subtitle: {
    fontSize: 20,
    color: COLORS.primary,
    textAlign: "center",
    marginTop: 16,
    lineHeight: 24,
    fontFamily: "Outfit_500Medium",
  },
  footer: {
    paddingBottom: 20,
  },
  continueButton: {
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    height: 60,
  },
  buttonText: {
    fontSize: 18,
    fontFamily: "Outfit_500Medium",
  },
  testButtons: {
    marginTop: 20,
    gap: 10,
  },
  testLabel: {
    fontSize: 14,
    color: COLORS.primary,
    fontFamily: "Outfit_600SemiBold",
    textAlign: "center",
    marginBottom: 5,
  },
  testButton: {
    backgroundColor: "rgba(26, 75, 74, 0.1)",
    paddingVertical: 12,
    paddingHorizontal: 20,
    borderRadius: 100,
    borderWidth: 1,
    borderColor: COLORS.primary,
  },
  testButtonText: {
    color: COLORS.primary,
    fontSize: 14,
    fontFamily: "Outfit_500Medium",
    textAlign: "center",
  },
});
