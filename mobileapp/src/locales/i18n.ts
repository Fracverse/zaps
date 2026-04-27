import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { getLocales } from "react-native-localize";

// Import translation files
import en from "./en.json";
import es from "./es.json";
import fr from "./fr.json";
import ar from "./ar.json";
import sw from "./sw.json";

const resources = {
  en: { translation: en },
  es: { translation: es },
  fr: { translation: fr },
  ar: { translation: ar },
  sw: { translation: sw },
};

const getDeviceLanguage = (): string => {
  const locales = getLocales();
  const deviceLanguage = locales[0]?.languageCode || "en";

  // Map device language to supported languages
  const languageMap: { [key: string]: string } = {
    en: "en",
    es: "es",
    fr: "fr",
    ar: "ar",
    sw: "sw",
  };

  return languageMap[deviceLanguage] || "en";
};

i18n.use(initReactI18next).init({
  resources,
  lng: getDeviceLanguage(),
  fallbackLng: "en",

  interpolation: {
    escapeValue: false, // React already escapes
  },

  react: {
    useSuspense: false, // Disable suspense mode
  },

  // Enable RTL for Arabic
  debug: __DEV__,
});

export { i18n };
export default i18n;
