import React from "react";
import { View, Text, StyleSheet, TouchableOpacity } from "react-native";
import { COLORS } from "../constants/colors";

interface AccountTypeCardProps {
    title: string;
    description: string;
    Icon: React.FC<any>;
    selected: boolean;
    onPress: () => void;
}

export const AccountTypeCard = ({
    title,
    description,
    Icon,
    selected,
    onPress,
}: AccountTypeCardProps) => {
    return (
        <TouchableOpacity
            style={[styles.card, selected && styles.cardSelected]}
            onPress={onPress}
            activeOpacity={0.8}
        >
            <View style={styles.iconContainer}>
                <Icon width={24} height={24} />
            </View>
            <View style={styles.textContainer}>
                <Text style={styles.cardTitle}>{title}</Text>
                <Text style={styles.cardDescription}>{description}</Text>
            </View>
        </TouchableOpacity>
    );
};

const styles = StyleSheet.create({
    card: {
        flexDirection: "row",
        alignItems: "center",
        backgroundColor: COLORS.white,
        borderWidth: 1,
        borderColor: "#E0E0E0",
        borderRadius: 100,
        minHeight: 100,
    },
    cardSelected: {
        borderColor: COLORS.primary,
        borderWidth: 1.5,
        backgroundColor: "#F0FDF4",
    },
    iconContainer: {
        flex: 0.3,
        minHeight: 100,
        alignItems: "center",
        justifyContent: "center",
        marginRight: 16,
        borderColor: "#EFEFEF",
        borderRightWidth: 1,
    },
    textContainer: {
        flex: 1,
    },
    cardTitle: {
        fontSize: 18,
        fontFamily: "Outfit_700Bold",
        color: COLORS.darkGray,
        marginBottom: 4,
    },
    cardDescription: {
        fontSize: 14,
        color: "#666",
        lineHeight: 20,
        fontFamily: "Outfit_400Regular",
    },
});
