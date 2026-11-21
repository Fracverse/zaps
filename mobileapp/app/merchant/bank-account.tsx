import React, { useCallback, useEffect, useState } from "react";
import {
  ActivityIndicator,
  FlatList,
  Modal,
  Pressable,
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  SafeAreaView,
  KeyboardAvoidingView,
  Platform,
  ScrollView,
} from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../../src/constants/colors";
import { useRouter } from "expo-router";
import { Input } from "../../src/components/Input";
import {
  BankAccount,
  SupportedBank,
  fetchBankAccount,
  fetchSupportedBanks,
  saveBankAccount,
  verifyBankAccount,
} from "../../src/services/bankAccountService";

export default function BankAccountScreen() {
  const router = useRouter();
  const [banks, setBanks] = useState<SupportedBank[]>([]);
  const [selectedBank, setSelectedBank] = useState<SupportedBank | null>(null);
  const [accountName, setAccountName] = useState("");
  const [accountNumber, setAccountNumber] = useState("");
  const [savedAccount, setSavedAccount] = useState<BankAccount | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isVerifying, setIsVerifying] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isLoadingBanks, setIsLoadingBanks] = useState(false);
  const [isBankPickerVisible, setBankPickerVisible] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [error, setError] = useState("");
  const [isVerified, setIsVerified] = useState(false);

  const loadScreen = useCallback(async () => {
    setIsLoading(true);
    setError("");
    const [bankResult, accountResult] = await Promise.allSettled([
      fetchSupportedBanks(),
      fetchBankAccount(),
    ]);
    const errors: string[] = [];
    if (bankResult.status === "fulfilled") {
      setBanks(bankResult.value.banks);
    } else {
      errors.push(
        bankResult.reason instanceof Error
          ? bankResult.reason.message
          : "Unable to load supported banks."
      );
    }
    if (accountResult.status === "fulfilled") {
      setSavedAccount(accountResult.value.bank_account);
    } else {
      errors.push(
        accountResult.reason instanceof Error
          ? accountResult.reason.message
          : "Unable to load bank account details."
      );
    }
    if (errors.length) setError(errors.join(" "));
    setIsLoading(false);
  }, []);

  useEffect(() => {
    void loadScreen();
  }, [loadScreen]);

  const openBankPicker = async () => {
    setError("");
    setBankPickerVisible(true);
    if (banks.length) return;
    setIsLoadingBanks(true);
    try {
      const response = await fetchSupportedBanks();
      setBanks(response.banks);
    } catch (loadError) {
      setError(
        loadError instanceof Error
          ? loadError.message
          : "Unable to load supported banks."
      );
    } finally {
      setIsLoadingBanks(false);
    }
  };

  const handleVerify = async () => {
    const number = accountNumber.replace(/\s/g, "");
    if (!selectedBank || !/^\d{10}$/.test(number)) {
      setError("Select a bank and enter a valid 10-digit account number.");
      return;
    }
    setError("");
    setIsVerifying(true);
    try {
      const result = await verifyBankAccount({
        bank_code: selectedBank.code,
        account_number: number,
      });
      setAccountName(result.account_name);
      setAccountNumber(number);
      setIsVerified(true);
    } catch (verifyError) {
      setIsVerified(false);
      setAccountName("");
      setError(
        verifyError instanceof Error
          ? verifyError.message
          : "We couldn't verify this account. Check the details and try again."
      );
    } finally {
      setIsVerifying(false);
    }
  };

  const handleSave = async () => {
    if (!selectedBank || !isVerified || isSaving) return;
    setError("");
    setIsSaving(true);
    try {
      const response = await saveBankAccount({
        bank_code: selectedBank.code,
        account_number: accountNumber.replace(/\s/g, ""),
      });
      setSavedAccount(response.bank_account);
      setIsEditing(false);
      setIsVerified(false);
    } catch (saveError) {
      setError(
        saveError instanceof Error
          ? saveError.message
          : "Unable to link this bank account. Please try again."
      );
    } finally {
      setIsSaving(false);
    }
  };

  const beginEditing = () => {
    setSelectedBank(null);
    setAccountName("");
    setAccountNumber("");
    setIsVerified(false);
    setIsEditing(true);
    setError("");
  };

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Bank Account</Text>
        <View style={{ width: 24 }} />
      </View>

      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : "height"}
        style={styles.content}
      >
        {isLoading ? (
          <View style={styles.centered}>
            <ActivityIndicator color={COLORS.primary} />
            <Text style={styles.statusText}>Loading bank account…</Text>
          </View>
        ) : savedAccount && !isEditing ? (
          <View style={styles.savedContainer}>
            <View style={styles.savedIcon}>
              <Ionicons name="checkmark" size={24} color={COLORS.white} />
            </View>
            <Text style={styles.savedTitle}>Bank account linked</Text>
            <Text style={styles.savedName}>{savedAccount.account_name}</Text>
            <Text style={styles.savedDetail}>
              {savedAccount.bank_name} · ••••••
              {savedAccount.account_number_last4}
            </Text>
            {error ? (
              <Text style={styles.errorText} accessibilityRole="alert">
                {error}
              </Text>
            ) : null}
            <TouchableOpacity
              style={styles.secondaryButton}
              onPress={beginEditing}
              accessibilityRole="button"
            >
              <Text style={styles.secondaryButtonText}>
                Change bank account
              </Text>
            </TouchableOpacity>
          </View>
        ) : (
          <>
            <ScrollView
              style={styles.formScroll}
              contentContainerStyle={styles.formContainer}
              keyboardShouldPersistTaps="handled"
            >
              <TouchableOpacity
                style={styles.selectField}
                onPress={() => void openBankPicker()}
                accessibilityRole="button"
                accessibilityLabel={`Bank: ${selectedBank?.name ?? "Select bank"}`}
              >
                <Text
                  style={[
                    styles.selectText,
                    !selectedBank && styles.placeholderText,
                  ]}
                >
                  {selectedBank?.name ?? "Select bank"}
                </Text>
                <Ionicons name="chevron-down" size={20} color={COLORS.black} />
              </TouchableOpacity>

              <Input
                label="Account number"
                placeholder="Enter 10-digit account number"
                value={accountNumber}
                onChangeText={(value) => {
                  setAccountNumber(value.replace(/\D/g, "").slice(0, 10));
                  setIsVerified(false);
                  setAccountName("");
                  setError("");
                }}
                keyboardType="number-pad"
                maxLength={10}
                editable={!isVerifying && !isSaving}
                testID="bank-account-number"
              />

              <Input
                label="Account name"
                placeholder="Shown after account verification"
                value={accountName}
                editable={false}
                testID="bank-account-name"
              />
              {error ? (
                <Text style={styles.errorText} accessibilityRole="alert">
                  {error}
                </Text>
              ) : null}
            </ScrollView>

            <View style={styles.bottomContainer}>
              {isVerified ? (
                <TouchableOpacity
                  style={[
                    styles.linkedButton,
                    isSaving && styles.disabledButton,
                  ]}
                  activeOpacity={0.8}
                  onPress={() => void handleSave()}
                  disabled={isSaving}
                  accessibilityRole="button"
                  accessibilityLabel={
                    isSaving ? "Linking bank account" : "Link account"
                  }
                  accessibilityState={{ disabled: isSaving }}
                >
                  {isSaving ? (
                    <ActivityIndicator color={COLORS.secondary} />
                  ) : (
                    <Text style={styles.linkedButtonText}>Link account</Text>
                  )}
                </TouchableOpacity>
              ) : (
                <TouchableOpacity
                  style={[
                    styles.linkedButton,
                    (!selectedBank ||
                      accountNumber.length !== 10 ||
                      isVerifying) &&
                      styles.disabledButton,
                  ]}
                  activeOpacity={0.8}
                  onPress={() => void handleVerify()}
                  disabled={
                    !selectedBank || accountNumber.length !== 10 || isVerifying
                  }
                  accessibilityRole="button"
                  accessibilityLabel={
                    isVerifying ? "Verifying bank account" : "Verify account"
                  }
                  accessibilityState={{
                    disabled:
                      !selectedBank ||
                      accountNumber.length !== 10 ||
                      isVerifying,
                  }}
                >
                  {isVerifying ? (
                    <ActivityIndicator color={COLORS.secondary} />
                  ) : (
                    <Text style={styles.linkedButtonText}>Verify account</Text>
                  )}
                </TouchableOpacity>
              )}
            </View>
          </>
        )}
      </KeyboardAvoidingView>

      <Modal
        visible={isBankPickerVisible}
        transparent
        animationType="slide"
        onRequestClose={() => setBankPickerVisible(false)}
      >
        <Pressable
          style={styles.modalBackdrop}
          onPress={() => setBankPickerVisible(false)}
        >
          <Pressable style={styles.bankSheet} onPress={() => undefined}>
            <View style={styles.sheetHeader}>
              <Text style={styles.sheetTitle}>Select bank</Text>
              <TouchableOpacity
                onPress={() => setBankPickerVisible(false)}
                accessibilityLabel="Close bank selector"
              >
                <Ionicons name="close" size={24} color={COLORS.black} />
              </TouchableOpacity>
            </View>
            {isLoadingBanks ? (
              <ActivityIndicator
                style={styles.bankLoading}
                color={COLORS.primary}
              />
            ) : error && banks.length === 0 ? (
              <Text style={styles.errorText} accessibilityRole="alert">
                {error}
              </Text>
            ) : (
              <FlatList
                data={banks}
                keyExtractor={(item) => item.code}
                renderItem={({ item }) => (
                  <TouchableOpacity
                    style={styles.bankOption}
                    onPress={() => {
                      setSelectedBank(item);
                      setIsVerified(false);
                      setAccountName("");
                      setError("");
                      setBankPickerVisible(false);
                    }}
                    accessibilityRole="button"
                  >
                    <Text style={styles.bankOptionText}>{item.name}</Text>
                    {selectedBank?.code === item.code ? (
                      <Ionicons
                        name="checkmark"
                        size={20}
                        color={COLORS.primary}
                      />
                    ) : null}
                  </TouchableOpacity>
                )}
                ListEmptyComponent={
                  <Text style={styles.emptyText}>
                    No supported banks found.
                  </Text>
                }
              />
            )}
          </Pressable>
        </Pressable>
      </Modal>
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
  content: {
    flex: 1,
    justifyContent: "space-between",
  },
  formScroll: {
    flex: 1,
  },
  formContainer: {
    paddingHorizontal: 20,
    paddingTop: 30,
    paddingBottom: 24,
  },
  selectField: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 24,
    paddingVertical: 18,
    borderRadius: 100,
    borderWidth: 1,
    borderColor: "#E0E0E0",
    backgroundColor: COLORS.white,
  },
  selectText: {
    fontSize: 16,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
  },
  placeholderText: {
    color: "#999",
  },

  bottomContainer: {
    paddingHorizontal: 20,
    paddingBottom: 30,
  },
  centered: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    gap: 12,
  },
  statusText: {
    fontSize: 15,
    color: "#666",
    fontFamily: "Outfit_400Regular",
  },
  savedContainer: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 24,
  },
  savedIcon: {
    width: 52,
    height: 52,
    borderRadius: 26,
    backgroundColor: COLORS.primary,
    alignItems: "center",
    justifyContent: "center",
    marginBottom: 16,
  },
  savedTitle: {
    color: COLORS.black,
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    marginBottom: 12,
  },
  savedName: {
    color: COLORS.black,
    fontSize: 17,
    fontFamily: "Outfit_600SemiBold",
    marginBottom: 6,
  },
  savedDetail: {
    color: "#666",
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    marginBottom: 24,
  },
  secondaryButton: {
    paddingVertical: 12,
    paddingHorizontal: 20,
  },
  secondaryButtonText: {
    color: COLORS.primary,
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
  },
  errorText: {
    color: "#B42318",
    fontSize: 14,
    fontFamily: "Outfit_500Medium",
    marginTop: -8,
  },
  disabledButton: {
    opacity: 0.5,
  },
  linkedButton: {
    width: "100%",
    height: 56,
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    justifyContent: "center",
    alignItems: "center",
  },
  linkedButtonText: {
    fontSize: 18,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.secondary,
  },
  modalBackdrop: {
    flex: 1,
    justifyContent: "flex-end",
    backgroundColor: "rgba(0,0,0,0.4)",
  },
  bankSheet: {
    maxHeight: "75%",
    backgroundColor: COLORS.white,
    borderTopLeftRadius: 24,
    borderTopRightRadius: 24,
    paddingHorizontal: 20,
    paddingBottom: 24,
  },
  sheetHeader: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingVertical: 20,
  },
  sheetTitle: {
    color: COLORS.black,
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
  },
  bankOption: {
    minHeight: 52,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: "#E0E0E0",
  },
  bankOptionText: {
    color: COLORS.black,
    fontSize: 16,
    fontFamily: "Outfit_400Regular",
  },
  bankLoading: {
    paddingVertical: 36,
  },
  emptyText: {
    color: "#666",
    textAlign: "center",
    paddingVertical: 24,
  },
});
