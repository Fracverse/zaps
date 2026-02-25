import React from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../src/constants/colors";
import { useRouter } from "expo-router";

const Section = ({ title, children, icon }: any) => (
  <View style={styles.section}>
    <View style={styles.sectionHeader}>
      <View style={styles.iconContainer}>
        <Ionicons name={icon} size={20} color={COLORS.primary} />
      </View>
      <Text style={styles.sectionTitle}>{title}</Text>
    </View>
    <Text style={styles.sectionText}>{children}</Text>
  </View>
);

export default function AboutBlinksScreen() {
  const router = useRouter();

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>About Blinks</Text>
        <View style={{ width: 24 }} />
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        {/* Hero Section */}
        <View style={styles.heroSection}>
          <Text style={styles.heroTitle}>Blinks</Text>
          <Text style={styles.heroSubtitle}>Fast. Secure. Seamless.</Text>
          <Text style={styles.heroDescription}>
            Blinks is a next-generation payment and transfer platform designed to 
            make moving money as easy as sending a message. Whether you're a user 
            sending funds to friends or a merchant accepting payments, Blinks 
            provides the speed and reliability you need.
          </Text>
        </View>

        {/* Mission Section */}
        <Section title="Our Mission" icon="rocket-outline">
          Our mission is to democratize financial access by providing a unified, 
          borderless payment experience. We believe that everyone should have 
          access to fast, low-cost financial services, regardless of location.
        </Section>

        {/* How It Works Section */}
        <Section title="How It Works" icon="settings-outline">
          Blinks leverages advanced blockchain technology to ensure near-instant 
          settlements. Simply scan a QR code, enter a Blinks ID, or tap to pay. 
          Our intelligent routing system handles the rest, ensuring your funds 
          reach their destination safely and efficiently.
        </Section>

        {/* Security & Transparency Section */}
        <Section title="Security & Transparency" icon="shield-checkmark-outline">
          Security is at the heart of everything we do. Blinks uses 
          multi-signature wallets, bank-grade encryption, and real-time 
          monitoring to protect your assets. All transactions are transparently 
          recorded on-chain, providing an immutable audit trail.
        </Section>

        {/* Call-to-Action Section */}
        <View style={styles.ctaCard}>
          <Text style={styles.ctaTitle}>Ready to start?</Text>
          <Text style={styles.ctaText}>
            Join thousands of users and merchants already using Blinks for their 
            daily transactions.
          </Text>
          <TouchableOpacity
            style={styles.ctaButton}
            onPress={() => router.replace("/(personal)/home")}
          >
            <Text style={styles.ctaButtonText}>Create a Blink</Text>
            <Ionicons name="arrow-forward" size={18} color={COLORS.white} />
          </TouchableOpacity>
        </View>

        <View style={styles.footer}>
          <Text style={styles.footerText}>© 2026 Blinks. All rights reserved.</Text>
        </View>
      </ScrollView>
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
    padding: 5,
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  scrollContent: {
    paddingHorizontal: 20,
    paddingTop: 10,
    paddingBottom: 30,
  },
  heroSection: {
    backgroundColor: COLORS.secondary,
    borderRadius: 24,
    padding: 30,
    alignItems: "center",
    marginBottom: 24,
  },
  heroTitle: {
    fontSize: 36,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    marginBottom: 4,
  },
  heroSubtitle: {
    fontSize: 18,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
    marginBottom: 16,
    opacity: 0.8,
  },
  heroDescription: {
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    color: COLORS.primary,
    textAlign: "center",
    lineHeight: 22,
  },
  section: {
    marginBottom: 24,
  },
  sectionHeader: {
    flexDirection: "row",
    alignItems: "center",
    marginBottom: 8,
    gap: 10,
  },
  iconContainer: {
    width: 36,
    height: 36,
    borderRadius: 18,
    backgroundColor: "#F5F5F5",
    justifyContent: "center",
    alignItems: "center",
  },
  sectionTitle: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  sectionText: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#444",
    lineHeight: 22,
  },
  ctaCard: {
    backgroundColor: COLORS.primary,
    borderRadius: 24,
    padding: 24,
    alignItems: "center",
    marginTop: 10,
  },
  ctaTitle: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.white,
    marginBottom: 8,
  },
  ctaText: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: COLORS.white,
    textAlign: "center",
    opacity: 0.9,
    marginBottom: 20,
    lineHeight: 20,
  },
  ctaButton: {
    backgroundColor: COLORS.black,
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 24,
    paddingVertical: 14,
    borderRadius: 12,
    gap: 8,
  },
  ctaButtonText: {
    color: COLORS.white,
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
  },
  footer: {
    marginTop: 40,
    alignItems: "center",
    paddingBottom: 20,
  },
  footerText: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
});
