import React, { useEffect, useState } from "react";
import { View, Text, StyleSheet, Animated } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Stack, useRouter } from "expo-router";

import Icon1 from "../assets/icon-1.svg";
import Icon2 from "../assets/icon-2.svg";
import Icon3 from "../assets/icon-3.svg";
import BlinkLogo from "../assets/blinkLogo.svg";

export default function OnboardingScreen() {
  const router = useRouter();
  const [showSplash, setShowSplash] = useState(true);

  useEffect(() => {
    const timer = setTimeout(() => {
      setShowSplash(false);
    }, 5000); // 5 seconds splash

    return () => clearTimeout(timer);
  }, []);

  if (showSplash) {
    return (
      <SafeAreaView style={styles.splashContainer}>
        <Stack.Screen options={{ headerShown: false }} />
        <View style={styles.splashContent}>
          <BlinkLogo width={216} height={103} style={styles.splashLogo} />
          <Text style={styles.splashText}>BLINK</Text>
        </View>
      </SafeAreaView>
    );
  }

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.content}>
        <View style={styles.header}>
          <BlinkLogo width={116} height={53} style={styles.splashLogo} />
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
            onPress={() => router.push("/onboarding-start")}
            variant="primary"
            style={styles.continueButton}
            textStyle={styles.buttonText}
          />
        </View>
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  splashContainer: {
    flex: 1,
    backgroundColor: COLORS.secondary,
    justifyContent: "center",
    alignItems: "center",
  },
  splashContent: {
    alignItems: "center",
    gap: 10, // Added gap as requested
  },
  splashLogo: {
    // width/height handled by SVG props
    marginBottom: 0, // Reset margin since using gap
  },
  splashText: {
    fontSize: 80, // Increased to 80px
    fontFamily: "Anton_400Regular",
    color: COLORS.primary,
    letterSpacing: 4,
    textTransform: "uppercase",
  },
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
});
