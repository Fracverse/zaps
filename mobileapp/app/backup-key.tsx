import React, { useState } from 'react';
import { View, Text, StyleSheet, TouchableOpacity, Clipboard } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Stack, useRouter } from 'expo-router';
import { COLORS } from '../src/constants/colors';
import { Button } from '../src/components/Button';
import { Ionicons } from '@expo/vector-icons';

export default function BackupKeyScreen() {
    const router = useRouter();
    const [hasBackedUp, setHasBackedUp] = useState(false);

    // Mock secret key
    const secretKey = "SCZANGBA5YHTNYVVV33H6MNWUQY7CZJM2ZCQMFRPY2DXNYBM6BKMB5M7";

    const handleCopy = () => {
        Clipboard.setString(secretKey);
        // Could show a toast or feedback here
    };

    const handleContinue = () => {
        if (hasBackedUp) {
            // Navigate to main app (e.g., tabs) or completion screen
            // For now, let's just go back to root or a placeholder
            router.push('/');
            alert('Wallet Setup Complete!');
        }
    };

    return (
        <SafeAreaView style={styles.container}>
            <Stack.Screen options={{ headerShown: false }} />

            <View style={styles.header}>
                <TouchableOpacity style={styles.backButton} onPress={() => router.back()}>
                    <Ionicons name="arrow-back" size={24} color={COLORS.black} />
                </TouchableOpacity>
                <Text style={styles.headerTitle}>Backup Your Secret Key</Text>
                <View style={{ width: 24 }} />
            </View>

            <View style={styles.content}>
                <Text style={styles.description}>
                    Save this key in a secure location. You'll need it to recover your wallet.
                </Text>

                <View style={styles.keyContainer}>
                    <Text style={styles.keyLabel}>Your Secret Key</Text>
                    <Text style={styles.keyValue}>{secretKey}</Text>
                </View>

                <TouchableOpacity style={styles.copyButton} onPress={handleCopy} activeOpacity={0.8}>
                    <Ionicons name="copy-outline" size={20} color={COLORS.black} style={{ marginRight: 8 }} />
                    <Text style={styles.copyButtonText}>Copy to Clipboard</Text>
                </TouchableOpacity>

                <TouchableOpacity
                    style={styles.checkboxContainer}
                    activeOpacity={0.8}
                    onPress={() => setHasBackedUp(!hasBackedUp)}
                >
                    <View style={[styles.checkbox, hasBackedUp && styles.checkboxChecked]}>
                        {hasBackedUp && <Ionicons name="checkmark" size={14} color={COLORS.primary} />}
                    </View>
                    <Text style={styles.checkboxText}>
                        I have saved my secret key in a secure location and understand I cannot recover it if lost.
                    </Text>
                </TouchableOpacity>
            </View>

            <View style={styles.footer}>
                <Button
                    title="I've backed up my key"
                    onPress={handleContinue}
                    variant="primary"
                    style={hasBackedUp ? {} : styles.disabledButton}
                // disabled={!hasBackedUp} // Button handles opacity style, but logic handled in onPress or here
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
        flexDirection: 'row',
        alignItems: 'center',
        justifyContent: 'space-between',
        paddingHorizontal: 20,
        paddingVertical: 10,
    },
    backButton: {
        padding: 8,
    },
    headerTitle: {
        fontSize: 20,
        fontFamily: 'Outfit_700Bold',
        color: COLORS.black,
    },
    content: {
        flex: 1,
        paddingHorizontal: 20,
        paddingTop: 20,
    },
    description: {
        fontSize: 16,
        color: '#666',
        marginBottom: 24,
        fontFamily: 'Outfit_400Regular',
        lineHeight: 22,
    },
    keyContainer: {
        backgroundColor: '#FAFAFA',
        borderRadius: 20,
        padding: 24,
        borderWidth: 1,
        borderColor: '#F0F0F0',
        marginBottom: 24,
    },
    keyLabel: {
        fontSize: 16,
        fontFamily: 'Outfit_700Bold',
        color: COLORS.black,
        marginBottom: 8,
    },
    keyValue: {
        fontSize: 16,
        fontFamily: 'Outfit_500Medium',
        color: '#666',
        lineHeight: 24,
        textTransform: 'uppercase',
    },
    copyButton: {
        backgroundColor: '#F5F5F5',
        borderRadius: 100,
        height: 56,
        flexDirection: 'row',
        alignItems: 'center',
        justifyContent: 'center',
        marginBottom: 32,
    },
    copyButtonText: {
        fontSize: 16,
        fontFamily: 'Outfit_600SemiBold',
        color: COLORS.black,
    },
    checkboxContainer: {
        flexDirection: 'row',
        backgroundColor: '#F5F5F5',
        borderRadius: 20,
        padding: 20,
        alignItems: 'flex-start',
    },
    checkbox: {
        width: 24,
        height: 24,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: '#999',
        marginRight: 16,
        justifyContent: 'center',
        alignItems: 'center',
        backgroundColor: COLORS.white,
        marginTop: 2,
    },
    checkboxChecked: {
        borderColor: COLORS.primary,
        // backgroundColor: COLORS.secondary, // Optional
    },
    checkboxText: {
        flex: 1,
        fontSize: 14,
        fontFamily: 'Outfit_400Regular',
        color: '#333',
        lineHeight: 20,
    },
    footer: {
        padding: 20,
        paddingBottom: 40,
    },
    disabledButton: {
        opacity: 0.5,
    }
});
