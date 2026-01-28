import React, { useState } from "react";
import {
    View,
    Text,
    StyleSheet,
    TouchableOpacity,
    ScrollView,
    LayoutAnimation,
    Platform,
    UIManager,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter, Stack } from "expo-router";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Input } from "../src/components/Input";
import { AccountTypeCard } from "../src/components/AccountTypeCard";

import ZapIcon from "../assets/icon-4.svg";
import WalletIcon from "../assets/wallet.svg";
import XLMLogo from "../assets/XML-logo.svg";
import USDTLogo from "../assets/USDT-logo.svg";
import USDCLogo from "../assets/USDC-logo.svg";

if (
    Platform.OS === "android" &&
    UIManager.setLayoutAnimationEnabledExperimental
) {
    UIManager.setLayoutAnimationEnabledExperimental(true);
}

const TOKENS = [
    { id: "xlm", symbol: "XLM", name: "Stellar", balance: "100.00", value: "125.32", Icon: XLMLogo },
    { id: "usdt", symbol: "USDT", name: "Tether", balance: "100.00", value: "100", Icon: USDTLogo },
    { id: "usdc", symbol: "USDC", name: "USD Coin", balance: "100.00", value: "100", Icon: USDCLogo },
];

const TokenSelectCard = ({ symbol, balance, value, Icon, selected, onPress }: any) => (
    <TouchableOpacity
        style={[styles.tokenCard, selected && styles.tokenCardSelected]}
        onPress={onPress}
        activeOpacity={0.8}
    >
        <View style={styles.tokenIcon}>
            <Icon width={32} height={32} />
        </View>
        <View style={styles.tokenInfo}>
            <Text style={styles.tokenSymbol}>{symbol}</Text>
            <Text style={styles.tokenBalance}>{balance}</Text>
        </View>
        <Text style={styles.tokenValue}>${value}</Text>
    </TouchableOpacity>
);

export default function TransferScreen() {
    const router = useRouter();
    const [step, setStep] = useState(0);
    const [transferType, setTransferType] = useState<"zaps" | "external" | null>(
        null
    );
    const [recipient, setRecipient] = useState("");
    const [amount, setAmount] = useState("");
    const [selectedToken, setSelectedToken] = useState(TOKENS[0].id);

    const token = TOKENS.find(t => t.id === selectedToken) || TOKENS[0];

    const handleNext = () => {
        if (step === 3) {
            router.replace("/(personal)/home");
            return;
        }
        LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
        setStep(step + 1);
    };

    const handleBack = () => {
        if (step === 0) {
            router.back();
        } else if (step === 3) {
            // Can't go back from success usually, but let's just go home
            router.replace("/(personal)/home");
        } else {
            LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
            setStep(step - 1);
        }
    };

    const renderStep0 = () => (
        <View style={styles.stepContainer}>
            <Text style={styles.subtitle}>Choose how you want to send money.</Text>
            <View style={styles.cardsContainer}>
                <AccountTypeCard
                    title="Blinx User"
                    description="Send instantly to any Blinx user via their Zaps ID"
                    Icon={ZapIcon}
                    selected={transferType === "zaps"}
                    onPress={() => setTransferType("zaps")}
                />
                <AccountTypeCard
                    title="External Wallet"
                    description="Send to any XLM or Stellar compatible wallet address"
                    Icon={WalletIcon}
                    selected={transferType === "external"}
                    onPress={() => setTransferType("external")}
                />
            </View>
        </View>
    );

    const renderStep1 = () => (
        <View style={styles.stepContainer}>
            <View style={styles.inputsSection}>
                <Input
                    placeholder={transferType === "zaps" ? "Recipient Zaps ID" : "Wallet Address"}
                    value={recipient}
                    onChangeText={setRecipient}
                    autoCapitalize="none"
                    style={styles.transferInput}
                />

                <Input
                    placeholder="Amount"
                    value={amount}
                    onChangeText={setAmount}
                    keyboardType="numeric"
                    style={styles.transferInput}
                />
            </View>

            <View style={styles.payWithSection}>
                <Text style={styles.payWithLabel}>Pay with</Text>
                <View style={styles.tokenList}>
                    {TOKENS.map((token) => (
                        <TokenSelectCard
                            key={token.id}
                            {...token}
                            selected={selectedToken === token.id}
                            onPress={() => setSelectedToken(token.id)}
                        />
                    ))}
                </View>
            </View>
        </View>
    );

    const renderStep2 = () => (
        <View style={styles.stepContainer}>
            <View style={styles.summaryCardLarge}>
                <View style={styles.summaryIconLarge}>
                    <token.Icon width={60} height={60} />
                </View>
                <Text style={styles.summaryAmountText}>{amount} {token.symbol}</Text>
                <Text style={styles.summaryFiatText}>$100</Text>

                <View style={styles.divider} />

                <View style={styles.infoRow}>
                    <View style={styles.recipientBadge}>
                        <ZapIcon width={16} height={16} />
                    </View>
                    <View style={styles.infoCol}>
                        <Text style={styles.infoLabel}>Recipient ID</Text>
                        <Text style={styles.infoValue}>{recipient}</Text>
                    </View>
                </View>

                <View style={[styles.infoRow, { marginTop: 24 }]}>
                    <View style={styles.infoCol}>
                        <Text style={styles.infoLabel}>Date</Text>
                    </View>
                    <Text style={styles.infoValueRight}>Nov 12 2025, 8.12 AM</Text>
                </View>
            </View>
        </View>
    );

    const renderStep3 = () => (
        <View style={[styles.stepContainer, styles.centerContent]}>
            <View style={styles.successOuter}>
                <View style={[styles.successRing, { width: 220, height: 220, opacity: 0.4 }]} />
                <View style={[styles.successRing, { width: 180, height: 180, opacity: 0.4 }]} />
                <View style={styles.successCheck}>
                    <Ionicons name="checkmark" size={60} color="#0E4A47" />
                </View>
            </View>

            <Text style={styles.successTitle}>Transfer Successful</Text>

            <View style={styles.amountCapsule}>
                <Text style={styles.amountCapsuleText}>{amount} {token.symbol}</Text>
            </View>
        </View>
    );

    return (
        <SafeAreaView style={styles.container}>
            <Stack.Screen options={{ headerShown: false }} />

            {step < 3 && (
                <View style={styles.header}>
                    <TouchableOpacity onPress={handleBack} style={styles.backButton}>
                        <Ionicons name="arrow-back" size={24} color={COLORS.black} />
                    </TouchableOpacity>
                    <Text style={styles.headerTitle}>
                        {step === 2 ? "Summary & confirmation" : "Transfer"}
                    </Text>
                    <View style={{ width: 40 }} />
                </View>
            )}

            <ScrollView
                contentContainerStyle={[
                    styles.scrollContent,
                    step === 3 && { justifyContent: "center" }
                ]}
                showsVerticalScrollIndicator={false}
            >
                {step === 0 && renderStep0()}
                {step === 1 && renderStep1()}
                {step === 2 && renderStep2()}
                {step === 3 && renderStep3()}
            </ScrollView>

            <View style={styles.footer}>
                <Button
                    title={
                        step === 1 ? "Review" :
                            step === 2 ? "Transfer" :
                                step === 3 ? "Done" : "Continue"
                    }
                    onPress={handleNext}
                    disabled={
                        (step === 0 && !transferType) ||
                        (step === 1 && (!recipient || !amount)) ||
                        (step === 2 && false)
                    }
                    icon={step === 2 ? (
                        <Ionicons name="refresh-outline" size={20} color={COLORS.secondary} style={{ marginRight: 8, transform: [{ rotate: "45deg" }] }} />
                    ) : undefined}
                    style={{ backgroundColor: "#0E4A47" }}
                />
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
        width: 40,
        height: 40,
        borderRadius: 20,
        justifyContent: "center",
        alignItems: "center",
    },
    headerTitle: {
        fontSize: 20,
        fontFamily: "Outfit_700Bold",
        color: COLORS.black,
    },
    scrollContent: {
        paddingHorizontal: 20,
        paddingTop: 10,
        flexGrow: 1,
    },
    stepContainer: {
        flex: 1,
    },
    centerContent: {
        justifyContent: "center",
        alignItems: "center",
    },
    subtitle: {
        fontSize: 16,
        color: "#666",
        marginBottom: 24,
        fontFamily: "Outfit_500Medium",
    },
    cardsContainer: {
        gap: 16,
        marginBottom: 32,
    },
    inputsSection: {
        marginBottom: 32,
        gap: 12,
    },
    transferInput: {
        borderWidth: 1,
        borderColor: COLORS.gray,
        height: 64,
    },
    payWithSection: {
        flex: 1,
    },
    payWithLabel: {
        fontSize: 18,
        fontFamily: "Outfit_600SemiBold",
        color: COLORS.black,
        marginBottom: 16,
    },
    tokenList: {
        gap: 12,
    },
    tokenCard: {
        flexDirection: "row",
        alignItems: "center",
        padding: 16,
        borderRadius: 100,
        borderWidth: 1,
        borderColor: "#F0F0F0",
        backgroundColor: COLORS.white,
    },
    tokenCardSelected: {
        borderColor: COLORS.primary,
        backgroundColor: "#F0FDF4",
    },
    tokenIcon: {
        width: 48,
        height: 48,
        borderRadius: 24,
        backgroundColor: "#F5F5F5",
        justifyContent: "center",
        alignItems: "center",
        marginRight: 12,
    },
    tokenInfo: {
        flex: 1,
    },
    tokenSymbol: {
        fontSize: 16,
        fontFamily: "Outfit_700Bold",
        color: COLORS.black,
    },
    tokenBalance: {
        fontSize: 14,
        fontFamily: "Outfit_400Regular",
        color: "#666",
    },
    tokenValue: {
        fontSize: 16,
        fontFamily: "Outfit_500Medium",
        color: COLORS.black,
    },
    summaryCardLarge: {
        backgroundColor: COLORS.white,
        borderRadius: 24,
        padding: 30,
        borderWidth: 1,
        borderColor: "#F0F0F0",
        alignItems: "center",
        marginTop: 20,
    },
    summaryIconLarge: {
        width: 100,
        height: 100,
        borderRadius: 50,
        backgroundColor: "#F5F5F5",
        justifyContent: "center",
        alignItems: "center",
        marginBottom: 20,
    },
    summaryAmountText: {
        fontSize: 28,
        fontFamily: "Outfit_700Bold",
        color: COLORS.black,
    },
    summaryFiatText: {
        fontSize: 18,
        fontFamily: "Outfit_500Medium",
        color: "#666",
        marginTop: 4,
    },
    divider: {
        height: 1,
        backgroundColor: "#F0F0F0",
        width: "100%",
        marginVertical: 30,
    },
    infoRow: {
        flexDirection: "row",
        alignItems: "center",
        width: "100%",
    },
    recipientBadge: {
        width: 40,
        height: 40,
        borderRadius: 20,
        backgroundColor: "#F5F5F5",
        justifyContent: "center",
        alignItems: "center",
        marginRight: 12,
    },
    infoCol: {
        flex: 1,
    },
    infoLabel: {
        fontSize: 12,
        fontFamily: "Outfit_400Regular",
        color: "#999",
    },
    infoValue: {
        fontSize: 16,
        fontFamily: "Outfit_600SemiBold",
        color: COLORS.black,
        marginTop: 2,
    },
    infoValueRight: {
        fontSize: 14,
        fontFamily: "Outfit_500Medium",
        color: COLORS.black,
    },
    successOuter: {
        width: 250,
        height: 250,
        justifyContent: "center",
        alignItems: "center",
        marginBottom: 40,
    },
    successRing: {
        position: "absolute",
        borderRadius: 999,
        borderWidth: 2,
        borderColor: "#EFEFEF",
    },
    successCheck: {
        width: 100,
        height: 100,
        borderRadius: 50,
        borderWidth: 4,
        borderColor: "#0E4A47",
        justifyContent: "center",
        alignItems: "center",
        backgroundColor: COLORS.white,
    },
    successTitle: {
        fontSize: 20,
        fontFamily: "Outfit_600SemiBold",
        color: COLORS.black,
        marginBottom: 20,
    },
    amountCapsule: {
        borderWidth: 1.5,
        borderColor: COLORS.black,
        borderRadius: 100,
        paddingHorizontal: 24,
        paddingVertical: 12,
    },
    amountCapsuleText: {
        fontSize: 24,
        fontFamily: "Outfit_700Bold",
        color: COLORS.black,
    },
    footer: {
        padding: 20,
        paddingBottom: Platform.OS === "ios" ? 40 : 20,
    },
});
