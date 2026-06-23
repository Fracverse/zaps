import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./en.json";
import es from "./es.json";
import fr from "./fr.json";
import ar from "./ar.json";
import sw from "./sw.json";

let cachedGetLocales: (() => { languageCode?: string }[]) | null | undefined;

function safeGetLocales(): (() => { languageCode?: string }[]) | null {
  if (cachedGetLocales === undefined) {
    try {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      cachedGetLocales = require("react-native-localize").getLocales;
    } catch {
      cachedGetLocales = null;
    }
  }
  return cachedGetLocales;
}

const resources = {
  en: { translation: en },
  es: { translation: es },
  fr: { translation: fr },
  ar: { translation: ar },
  sw: { translation: sw },
};

const getDeviceLanguage = (): string => {
  const getLocales = safeGetLocales();
  const deviceLanguage = getLocales
    ? getLocales()[0]?.languageCode || "en"
    : "en";

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
