import React from "react";
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  I18nManager,
} from "react-native";
import { useTranslation } from "react-i18next";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../constants/colors";

interface Language {
  code: string;
  name: string;
  nativeName: string;
  isRTL?: boolean;
}

const languages: Language[] = [
  { code: "en", name: "English", nativeName: "English" },
  { code: "es", name: "Spanish", nativeName: "Español" },
  { code: "fr", name: "French", nativeName: "Français" },
  { code: "ar", name: "Arabic", nativeName: "العربية", isRTL: true },
  { code: "sw", name: "Swahili", nativeName: "Kiswahili" },
];

export const LanguageSelector: React.FC<{
  onLanguageChange?: (languageCode: string) => void;
}> = ({ onLanguageChange }) => {
  const { i18n, t } = useTranslation();
  const currentLanguage = i18n.language;

  const handleLanguageChange = async (languageCode: string) => {
    try {
      const selectedLanguage = languages.find(
        (lang) => lang.code === languageCode
      );

      if (selectedLanguage?.isRTL !== I18nManager.isRTL) {
        // Force restart for RTL layout changes
        I18nManager.forceRTL(selectedLanguage?.isRTL || false);
      }

      await i18n.changeLanguage(languageCode);
      onLanguageChange?.(languageCode);

      // Show success message
      if (typeof global !== "undefined" && global.toast) {
        global.toast.success(t("common.success"));
      }
    } catch (error) {
      console.error("Language change failed:", error);
      if (typeof global !== "undefined" && global.toast) {
        global.toast.error(t("errors.generic"));
      }
    }
  };

  return (
    <View style={styles.container}>
      <Text style={styles.title}>{t("settings.language")}</Text>
      <View style={styles.languageList}>
        {languages.map((language) => (
          <TouchableOpacity
            key={language.code}
            style={[
              styles.languageItem,
              currentLanguage === language.code && styles.selectedLanguage,
            ]}
            onPress={() => handleLanguageChange(language.code)}
          >
            <View style={styles.languageInfo}>
              <Text
                style={[
                  styles.languageName,
                  currentLanguage === language.code && styles.selectedText,
                ]}
              >
                {language.nativeName}
              </Text>
              <Text style={styles.languageSecondary}>{language.name}</Text>
            </View>
            {currentLanguage === language.code && (
              <Ionicons
                name="checkmark-circle"
                size={20}
                color={COLORS.primary}
              />
            )}
          </TouchableOpacity>
        ))}
      </View>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    padding: 16,
  },
  title: {
    fontSize: 18,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.text,
    marginBottom: 16,
  },
  languageList: {
    gap: 8,
  },
  languageItem: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    padding: 16,
    backgroundColor: COLORS.gray,
    borderRadius: 12,
  },
  selectedLanguage: {
    backgroundColor: COLORS.background,
    borderWidth: 1,
    borderColor: COLORS.primary,
  },
  languageInfo: {
    flex: 1,
  },
  languageName: {
    fontSize: 16,
    fontFamily: "Outfit_500Medium",
    color: COLORS.text,
  },
  selectedText: {
    color: COLORS.primary,
    fontFamily: "Outfit_600SemiBold",
  },
  languageSecondary: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: COLORS.darkGray,
    marginTop: 2,
  },
});
