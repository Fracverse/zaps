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

const PolicySection = ({
  title,
  content,
}: {
  title: string;
  content: string;
}) => {
  return (
    <View style={styles.section}>
      <Text style={styles.sectionTitle}>{title}</Text>
      <Text style={styles.sectionContent}>{content}</Text>
    </View>
  );
};

export default function PrivacyPolicyScreen() {
  const router = useRouter();

  const sections = [
    {
      title: "1. Information We Collect",
      content:
        "We collect information you provide directly to us, such as when you create an account, complete a transaction, or contact support. This may include your name, email address, phone number, and financial information related to your transactions.",
    },
    {
      title: "2. How We Use Your Information",
      content:
        "We use the information we collect to provide, maintain, and improve our services, to process your transactions, to communicate with you, and to protect Blinks and our users.",
    },
    {
      title: "3. Information Sharing",
      content:
        "We do not share your personal information with third parties except as described in this policy, such as to comply with legal obligations, protect our rights, or with your consent.",
    },
    {
      title: "4. Data Security",
      content:
        "We take reasonable measures to help protect your information from loss, theft, misuse, and unauthorized access, disclosure, alteration, and destruction.",
    },
    {
      title: "5. Your Choices",
      content:
        "You can access and update your account information at any time. You may also contact us to request the deletion of your account and personal information.",
    },
    {
      title: "6. Changes to this Policy",
      content:
        "We may update this Privacy Policy from time to time. If we make changes, we will notify you by revising the date at the top of the policy.",
    },
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
        <Text style={styles.headerTitle}>Privacy Policy</Text>
        <View style={{ width: 24 }} />
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        <Text style={styles.title}>Privacy Policy</Text>
        <Text style={styles.lastUpdated}>Last Updated: February 23, 2026</Text>

        <Text style={styles.intro}>
          At Blinks, we are committed to protecting your privacy and ensuring
          you have a positive experience when using our services.
        </Text>

        <View style={styles.sectionsContainer}>
          {sections.map((section, index) => (
            <PolicySection
              key={index}
              title={section.title}
              content={section.content}
            />
          ))}
        </View>

        <View style={styles.contactInfo}>
          <Text style={styles.contactTitle}>Questions?</Text>
          <Text style={styles.contactText}>
            If you have any questions about this Privacy Policy, please contact
            us at privacy@blinks.com.
          </Text>
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
  title: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 4,
  },
  lastUpdated: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    marginBottom: 20,
  },
  intro: {
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    color: "#444",
    lineHeight: 22,
    marginBottom: 30,
  },
  sectionsContainer: {
    gap: 24,
  },
  section: {
    gap: 8,
  },
  sectionTitle: {
    fontSize: 17,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  sectionContent: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#555",
    lineHeight: 22,
  },
  contactInfo: {
    marginTop: 40,
    padding: 20,
    backgroundColor: "#F9F9F9",
    borderRadius: 16,
    borderWidth: 1,
    borderColor: "#F0F0F0",
  },
  contactTitle: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 8,
  },
  contactText: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    lineHeight: 20,
  },
});
