import React from 'react';
import { TextInput, View, Text, StyleSheet, TextInputProps } from 'react-native';
import { COLORS } from '../constants/colors';

interface InputProps extends TextInputProps {
    label?: string;
    error?: string;
}

export const Input = ({ label, error, style, ...props }: InputProps) => {
    return (
        <View style={styles.container}>
            {label && <Text style={styles.label}>{label}</Text>}
            <TextInput
                style={[
                    styles.input,
                    error ? styles.inputError : null,
                    style
                ]}
                placeholderTextColor="#999"
                {...props}
            />
            {error && <Text style={styles.errorText}>{error}</Text>}
        </View>
    );
};

const styles = StyleSheet.create({
    container: {
        marginBottom: 16,
        width: '100%',
    },
    label: {
        fontSize: 16,
        fontFamily: 'Outfit_600SemiBold',
        color: '#000',
        marginBottom: 8,
    },
    input: {
        height: 56,
        backgroundColor: '#FFFFFF',
        borderRadius: 28, // Fully rounded as per design
        paddingHorizontal: 24,
        fontSize: 16,
        fontFamily: 'Outfit_400Regular',
        color: COLORS.black,
        borderWidth: 1,
        borderColor: '#eee',

    },
    inputError: {
        borderColor: 'red',
    },
    errorText: {
        color: 'red',
        fontSize: 12,
        fontFamily: 'Outfit_400Regular',
        marginTop: 4,
        marginLeft: 12,
    },
});
