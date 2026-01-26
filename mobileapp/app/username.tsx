import React, { useState } from 'react';
import { View, Text, StyleSheet, TouchableOpacity } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Stack, useRouter } from 'expo-router';
import { COLORS } from '../src/constants/colors';
import { Button } from '../src/components/Button';
import { Input } from '../src/components/Input';
import { Ionicons } from '@expo/vector-icons';

export default function UsernameScreen() {
    const router = useRouter();
    const [username, setUsername] = useState('');

    const handleContinue = () => {
        if (username.length > 0) {
            router.push('/password');
        }
    };

    return (
        <SafeAreaView style={styles.container}>
            <Stack.Screen options={{ headerShown: false }} />

            <View style={styles.header}>
                <TouchableOpacity style={styles.backButton} onPress={() => router.back()}>
                    <Ionicons name="arrow-back" size={24} color={COLORS.black} />
                </TouchableOpacity>
                <Text style={styles.headerTitle}>Enter a username</Text>
                <View style={{ width: 24 }} />
            </View>

            <View style={styles.content}>
                <Text style={styles.subtitle}>Set up your personal account to get started.</Text>

                <View style={styles.inputContainer}>
                    <Input
                        placeholder="Username will be used as ID"
                        value={username}
                        onChangeText={setUsername}
                        autoCapitalize="none"
                        autoCorrect={false}
                    />
                </View>
            </View>

            <View style={styles.footer}>
                <Button
                    title="Continue"
                    onPress={handleContinue}
                    variant="primary"
                    style={username.length > 0 ? {} : styles.disabledButton}
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
    subtitle: {
        fontSize: 16,
        color: '#666',
        marginBottom: 32,
        fontFamily: 'Outfit_500Medium',
    },
    inputContainer: {
        width: '100%',
    },
    footer: {
        padding: 20,
        paddingBottom: 30,
    },
    disabledButton: {
        opacity: 0.5,
    }
});
