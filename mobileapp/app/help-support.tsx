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

const SupportItem = ({ icon, label, onPress, isLast }: any) => {
  return (
    <TouchableOpacity
      style={[styles.supportItem, isLast && styles.lastSupportItem]}
      onPress={onPress}
      activeOpacity={0.7}
    >
      <View style={styles.supportIcon}>
        <Ionicons name={icon} size={22} color={COLORS.black} />
      </View>
      <Text style={styles.supportLabel}>{label}</Text>
      <Ionicons name="chevron-forward" size={20} color={COLORS.black} />
    </TouchableOpacity>
  );
};

export default function HelpSupportScreen() {
  const router = useRouter();

  const supportOptions = [
    {
      id: "faq",
      icon: "help-circle-outline",
      label: "Frequently Asked Questions",
    },
    {
      id: "contact",
      icon: "chatbubble-ellipses-outline",
      label: "Contact Support",
    },
    {
      id: "privacy",
      icon: "shield-checkmark-outline",
      label: "Privacy Policy",
    },
    { id: "terms", icon: "document-text-outline", label: "Terms of Service" },
    { id: "about", icon: "information-circle-outline", label: "About Blinks" },
  ];

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Help & Support</Text>
        <View style={{ width: 24 }} />
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        <View style={styles.illustrationCard}>
          <View style={styles.iconCircle}>
            <Ionicons name="headset-outline" size={40} color={COLORS.primary} />
          </View>
          <Text style={styles.illustrationTitle}>How can we help you?</Text>
          <Text style={styles.illustrationDesc}>
            Our team is here to help you with any issues or questions you might
            have.
          </Text>
        </View>

        <View style={styles.supportList}>
          {supportOptions.map((option, index) => (
            <SupportItem
              key={option.id}
              icon={option.icon}
              label={option.label}
              onPress={() => {
                const routes: any = {
                  faq: "/faq",
                  contact: "/contact-support",
                  privacy: "/privacy-policy",
                  terms: "/terms-of-service",
                  about: "/about-blinks",
                };
                if (routes[option.id]) {
                  router.push(routes[option.id]);
                }
              }}
              isLast={index === supportOptions.length - 1}
            />
          ))}
        </View>
      </ScrollView>

      <View style={styles.footer}>
        <Text style={styles.versionText}>Blinks v1.0.0 (Build 124)</Text>
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
  illustrationCard: {
    backgroundColor: "#F9F9F9",
    borderRadius: 24,
    padding: 30,
    alignItems: "center",
    marginBottom: 30,
    borderWidth: 1,
    borderColor: "#F0F0F0",
  },
  iconCircle: {
    width: 80,
    height: 80,
    borderRadius: 40,
    backgroundColor: COLORS.secondary,
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 16,
  },
  illustrationTitle: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 8,
    textAlign: "center",
  },
  illustrationDesc: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    textAlign: "center",
    lineHeight: 20,
  },
  supportList: {
    backgroundColor: COLORS.white,
    borderRadius: 20,
    paddingVertical: 5,
  },
  supportItem: {
    flexDirection: "row",
    alignItems: "center",
    paddingVertical: 18,
    borderBottomWidth: 1,
    borderBottomColor: "#F0F0F0",
  },
  lastSupportItem: {
    borderBottomWidth: 0,
  },
  supportIcon: {
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: "#F5F5F5",
    justifyContent: "center",
    alignItems: "center",
    marginRight: 15,
  },
  supportLabel: {
    flex: 1,
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
  },
  footer: {
    paddingVertical: 20,
    alignItems: "center",
  },
  versionText: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
});
