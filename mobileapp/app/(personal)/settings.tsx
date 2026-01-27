import React, { useState } from 'react';
import { View, Text, StyleSheet, TouchableOpacity, ScrollView, Switch, SafeAreaView } from 'react-native';
import { Ionicons, MaterialCommunityIcons, Feather } from '@expo/vector-icons';
import { COLORS } from '../../src/constants/colors';
import { useRouter } from 'expo-router';

const SettingsItem = ({ icon, label, sublabel, hasToggle, value, onToggle, onPress, isLast }: any) => {
  return (
    <TouchableOpacity 
      style={[styles.settingsItem, isLast && styles.lastSettingsItem]} 
      onPress={onPress}
      activeOpacity={0.7}
      disabled={hasToggle}
    >
      <View style={styles.settingsIcon}>
        <Ionicons name={icon} size={22} color={COLORS.black} />
      </View>
      <View style={styles.settingsTextContent}>
        <Text style={styles.settingsLabel}>{label}</Text>
        {sublabel && <Text style={styles.settingsSublabel}>{sublabel}</Text>}
      </View>
      {hasToggle ? (
        <Switch
          value={value}
          onValueChange={onToggle}
          trackColor={{ false: '#E0E0E0', true: COLORS.secondary }}
          thumbColor={value ? COLORS.primary : '#F5F5F5'}
        />
      ) : (
        <Ionicons name="chevron-forward" size={20} color={COLORS.black} />
      )}
    </TouchableOpacity>
  );
};

export default function SettingsScreen() {
  const router = useRouter();
  const [notifications, setNotifications] = useState(true);
  const [biometrics, setBiometrics] = useState(true);

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.header}>
        <TouchableOpacity onPress={() => router.back()} style={styles.backButton}>
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Settings</Text>
        <View style={{ width: 24 }} />
      </View>

      <ScrollView contentContainerStyle={styles.scrollContent} showsVerticalScrollIndicator={false}>
        <View style={styles.profileCard}>
          <Text style={styles.profileName}>Ejembiii.zaps</Text>
          <View style={styles.addressRow}>
            <Text style={styles.addressText}>0x4A7d5cBe16...da79bB2cF9a1B</Text>
            <TouchableOpacity>
              <Ionicons name="copy-outline" size={18} color={COLORS.primary} />
            </TouchableOpacity>
          </View>
        </View>

        <View style={styles.settingsList}>
          <SettingsItem 
            icon="notifications-outline" 
            label="Notification" 
            sublabel="Enable or disable push notifications"
            hasToggle={true}
            value={notifications}
            onToggle={setNotifications}
          />
          <SettingsItem 
            icon="lock-closed-outline" 
            label="Password" 
            sublabel="Change Password"
            onPress={() => router.push('/(personal)/change-password')}
          />
          <SettingsItem 
            icon="finger-print-outline" 
            label="Biometrics" 
            sublabel="Use Face ID / Fingerprint"
            hasToggle={true}
            value={biometrics}
            onToggle={setBiometrics}
          />
          <SettingsItem 
            icon="language-outline" 
            label="Language" 
            sublabel="English"
            onPress={() => {}}
          />
          <SettingsItem 
            icon="help-circle-outline" 
            label="Help & Support" 
            sublabel="Reach out for assistance"
            onPress={() => {}}
            isLast={true}
          />
        </View>
      </ScrollView>
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
    paddingVertical: 15,
  },
  backButton: {
    padding: 5,
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: 'Outfit_700Bold',
    color: COLORS.black,
  },
  scrollContent: {
    paddingHorizontal: 20,
    paddingTop: 10,
    paddingBottom: 30,
  },
  profileCard: {
    backgroundColor: COLORS.white,
    borderRadius: 24,
    padding: 30,
    borderWidth: 1,
    borderColor: '#F0F0F0',
    alignItems: 'center',
    marginBottom: 30,
  },
  profileName: {
    fontSize: 32,
    fontFamily: 'Outfit_700Bold',
    color: COLORS.primary,
    marginBottom: 12,
  },
  addressRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
  },
  addressText: {
    fontSize: 16,
    fontFamily: 'Outfit_500Medium',
    color: COLORS.primary,
  },
  settingsList: {
    gap: 8,
  },
  settingsItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 15,
  },
  lastSettingsItem: {
    borderBottomWidth: 0,
  },
  settingsIcon: {
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: '#F5F5F5',
    justifyContent: 'center',
    alignItems: 'center',
    marginRight: 15,
  },
  settingsTextContent: {
    flex: 1,
  },
  settingsLabel: {
    fontSize: 16,
    fontFamily: 'Outfit_600SemiBold',
    color: COLORS.black,
  },
  settingsSublabel: {
    fontSize: 13,
    fontFamily: 'Outfit_400Regular',
    color: '#999',
    marginTop: 2,
  },
});
