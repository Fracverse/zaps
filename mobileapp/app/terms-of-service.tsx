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

const TermSection = ({
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

export default function TermsOfServiceScreen() {
  const router = useRouter();

  const sections = [
    {
      title: "1. Acceptance of Terms",
      content:
        "By accessing or using Blinks, you agree to be bound by these Terms of Service and all applicable laws and regulations. If you do not agree with any of these terms, you are prohibited from using or accessing this service.",
    },
    {
      title: "2. Use License",
      content:
        "Permission is granted to temporarily download one copy of the materials (information or software) on Blinks' website for personal, non-commercial transitory viewing only.",
    },
    {
      title: "3. Disclaimer",
      content:
        "The materials on Blinks are provided on an 'as is' basis. Blinks makes no warranties, expressed or implied, and hereby disclaims and negates all other warranties including, without limitation, implied warranties or conditions of merchantability, fitness for a particular purpose, or non-infringement of intellectual property or other violation of rights.",
    },
    {
      title: "4. Limitations",
      content:
        "In no event shall Blinks or its suppliers be liable for any damages (including, without limitation, damages for loss of data or profit, or due to business interruption) arising out of the use or inability to use the materials on Blinks.",
    },
    {
      title: "5. Accuracy of Materials",
      content:
        "The materials appearing on Blinks could include technical, typographical, or photographic errors. Blinks does not warrant that any of the materials on its website are accurate, complete or current.",
    },
    {
      title: "6. Links",
      content:
        "Blinks has not reviewed all of the sites linked to its website and is not responsible for the contents of any such linked site. The inclusion of any link does not imply endorsement by Blinks of the site.",
    },
    {
      title: "7. Modifications",
      content:
        "Blinks may revise these terms of service for its website at any time without notice. By using this website you are agreeing to be bound by the then current version of these terms of service.",
    },
    {
      title: "8. Governing Law",
      content:
        "These terms and conditions are governed by and construed in accordance with the laws of the jurisdiction in which Blinks operates and you irrevocably submit to the exclusive jurisdiction of the courts in that State or location.",
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
        <Text style={styles.headerTitle}>Terms of Service</Text>
        <View style={{ width: 24 }} />
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        <Text style={styles.title}>Terms of Service</Text>
        <Text style={styles.lastUpdated}>Last Updated: February 23, 2026</Text>

        <Text style={styles.intro}>
          Please read these Terms of Service carefully before using Blinks.
          These terms govern your access to and use of our platform.
        </Text>

        <View style={styles.sectionsContainer}>
          {sections.map((section, index) => (
            <TermSection
              key={index}
              title={section.title}
              content={section.content}
            />
          ))}
        </View>

        <View style={styles.contactInfo}>
          <Text style={styles.contactTitle}>Contact Us</Text>
          <Text style={styles.contactText}>
            If you have any questions about these Terms, please contact us at
            legal@blinks.com.
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
