import React, { useState } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  TextInput,
  KeyboardAvoidingView,
  Platform,
  Alert,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../src/constants/colors";
import { useRouter } from "expo-router";

const ContactMethod = ({ icon, label, sublabel, type }: any) => {
  return (
    <TouchableOpacity style={styles.methodCard} activeOpacity={0.7}>
      <View style={[styles.methodIcon, (styles as any)[type]]}>
        <Ionicons name={icon} size={24} color={COLORS.white} />
      </View>
      <View style={styles.methodText}>
        <Text style={styles.methodLabel}>{label}</Text>
        <Text style={styles.methodSublabel}>{sublabel}</Text>
      </View>
      <Ionicons name="chevron-forward" size={20} color="#CCC" />
    </TouchableOpacity>
  );
};

export default function ContactSupportScreen() {
  const router = useRouter();
  const [subject, setSubject] = useState("");
  const [message, setMessage] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSendMessage = () => {
    if (!subject || !message) {
      Alert.alert("Error", "Please fill in all fields.");
      return;
    }

    setLoading(true);
    // Simulate API call
    setTimeout(() => {
      setLoading(false);
      Alert.alert(
        "Message Sent",
        "Your message has been sent successfully. Our team will get back to you soon.",
        [{ text: "OK", onPress: () => router.back() }]
      );
    }, 1500);
  };

  return (
    <SafeAreaView style={styles.container}>
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : "height"}
        style={{ flex: 1 }}
      >
        <View style={styles.header}>
          <TouchableOpacity
            onPress={() => router.back()}
            style={styles.backButton}
          >
            <Ionicons name="arrow-back" size={24} color={COLORS.black} />
          </TouchableOpacity>
          <Text style={styles.headerTitle}>Contact Support</Text>
          <View style={{ width: 24 }} />
        </View>

        <ScrollView
          contentContainerStyle={styles.scrollContent}
          showsVerticalScrollIndicator={false}
        >
          <Text style={styles.title}>Get in Touch</Text>
          <Text style={styles.subtitle}>
            Have a question or feedback? We'd love to hear from you.
          </Text>

          <View style={styles.methodsContainer}>
            <ContactMethod
              icon="mail-outline"
              label="Email Us"
              sublabel="support@zaps.com"
              type="email"
            />
            <ContactMethod
              icon="call-outline"
              label="Call Us"
              sublabel="+234 812 345 6789"
              type="call"
            />
            <ContactMethod
              icon="logo-whatsapp"
              label="WhatsApp"
              sublabel="Chat with us anytime"
              type="whatsapp"
            />
          </View>

          <View style={styles.formContainer}>
            <Text style={styles.formTitle}>Send us a message</Text>
            
            <View style={styles.inputGroup}>
              <Text style={styles.label}>Subject</Text>
              <TextInput
                style={styles.input}
                placeholder="What is this regarding?"
                value={subject}
                onChangeText={setSubject}
                placeholderTextColor="#999"
              />
            </View>

            <View style={styles.inputGroup}>
              <Text style={styles.label}>Message</Text>
              <TextInput
                style={[styles.input, styles.textArea]}
                placeholder="Type your message here..."
                value={message}
                onChangeText={setMessage}
                multiline
                numberOfLines={6}
                textAlignVertical="top"
                placeholderTextColor="#999"
              />
            </View>

            <TouchableOpacity
              style={[styles.sendButton, loading && styles.disabledButton]}
              onPress={handleSendMessage}
              disabled={loading}
              activeOpacity={0.8}
            >
              <Text style={styles.sendButtonText}>
                {loading ? "Sending..." : "Send Message"}
              </Text>
            </TouchableOpacity>
          </View>
        </ScrollView>
      </KeyboardAvoidingView>
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
    marginBottom: 8,
  },
  subtitle: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    marginBottom: 24,
  },
  methodsContainer: {
    gap: 12,
    marginBottom: 32,
  },
  methodCard: {
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#F9F9F9",
    borderRadius: 16,
    padding: 16,
    borderWidth: 1,
    borderColor: "#F0F0F0",
  },
  methodIcon: {
    width: 48,
    height: 48,
    borderRadius: 24,
    justifyContent: "center",
    alignItems: "center",
    marginRight: 16,
  },
  methodText: {
    flex: 1,
  },
  methodLabel: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
  },
  methodSublabel: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    marginTop: 2,
  },
  email: { backgroundColor: "#5856D6" },
  call: { backgroundColor: COLORS.primary },
  whatsapp: { backgroundColor: "#25D366" },
  
  formContainer: {
    gap: 20,
  },
  formTitle: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  inputGroup: {
    gap: 8,
  },
  label: {
    fontSize: 14,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
  },
  input: {
    backgroundColor: "#F9F9F9",
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    paddingHorizontal: 16,
    height: 56,
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
  },
  textArea: {
    height: 120,
    paddingTop: 16,
  },
  sendButton: {
    backgroundColor: COLORS.primary,
    height: 56,
    borderRadius: 28,
    justifyContent: "center",
    alignItems: "center",
    marginTop: 10,
  },
  disabledButton: {
    opacity: 0.7,
  },
  sendButtonText: {
    color: COLORS.white,
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
  },
});
