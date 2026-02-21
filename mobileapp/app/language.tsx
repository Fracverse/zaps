import React, { useState } from "react";
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

const languages = [
  { id: "en", name: "English", flag: "🇺🇸" },
  { id: "es", name: "Spanish", flag: "🇪🇸" },
  { id: "fr", name: "French", flag: "🇫🇷" },
  { id: "de", name: "German", flag: "🇩🇪" },
  { id: "zh", name: "Chinese", flag: "🇨🇳" },
  { id: "ar", name: "Arabic", flag: "🇸🇦" },
];

export default function LanguageScreen() {
  const router = useRouter();
  const [selectedLanguage, setSelectedLanguage] = useState("en");

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Language</Text>
        <View style={{ width: 24 }} />
      </View>

      <ScrollView contentContainerStyle={styles.scrollContent}>
        <View style={styles.languageList}>
          {languages.map((lang, index) => {
            const isSelected = selectedLanguage === lang.id;
            return (
              <TouchableOpacity
                key={lang.id}
                style={[
                  styles.languageItem,
                  isSelected && styles.selectedLanguageItem,
                ]}
                onPress={() => setSelectedLanguage(lang.id)}
                activeOpacity={0.7}
              >
                <View style={styles.languageContent}>
                  <Text style={styles.flagText}>{lang.flag}</Text>
                  <Text
                    style={[
                      styles.languageName,
                      isSelected && styles.selectedLanguageName,
                    ]}
                  >
                    {lang.name}
                  </Text>
                </View>
                {isSelected ? (
                  <View style={styles.radioSelected}>
                    <View style={styles.radioInner} />
                  </View>
                ) : (
                  <View style={styles.radioUnselected} />
                )}
              </TouchableOpacity>
            );
          })}
        </View>
      </ScrollView>

      <View style={styles.footer}>
        <TouchableOpacity
          style={styles.saveButton}
          onPress={() => {
            // In a real app, this would save the selection to state/storage
            router.back();
          }}
        >
          <Text style={styles.saveButtonText}>Save Changes</Text>
        </TouchableOpacity>
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
  languageList: {
    gap: 12,
  },
  languageItem: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    padding: 16,
    borderRadius: 16,
    backgroundColor: "#F9F9F9",
    borderWidth: 1,
    borderColor: "#F0F0F0",
  },
  selectedLanguageItem: {
    backgroundColor: COLORS.white,
    borderColor: COLORS.primary,
    borderWidth: 2,
  },
  languageContent: {
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
  },
  flagText: {
    fontSize: 24,
  },
  languageName: {
    fontSize: 16,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
  },
  selectedLanguageName: {
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  radioUnselected: {
    width: 24,
    height: 24,
    borderRadius: 12,
    borderWidth: 2,
    borderColor: "#E0E0E0",
  },
  radioSelected: {
    width: 24,
    height: 24,
    borderRadius: 12,
    borderWidth: 2,
    borderColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
  },
  radioInner: {
    width: 12,
    height: 12,
    borderRadius: 6,
    backgroundColor: COLORS.primary,
  },
  footer: {
    padding: 20,
    borderTopWidth: 1,
    borderTopColor: "#F0F0F0",
  },
  saveButton: {
    backgroundColor: COLORS.primary,
    height: 56,
    borderRadius: 28,
    justifyContent: "center",
    alignItems: "center",
  },
  saveButtonText: {
    color: COLORS.white,
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
  },
});
