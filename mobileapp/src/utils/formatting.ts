import { useTranslation } from 'react-i18next';

// Currency formatting based on locale
export const formatCurrency = (
  amount: number,
  currency: string = 'USD',
  locale?: string
): string => {
  const { i18n } = useTranslation();
  const targetLocale = locale || i18n.language;
  
  try {
    return new Intl.NumberFormat(targetLocale, {
      style: 'currency',
      currency: currency,
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(amount);
  } catch (error) {
    // Fallback for unsupported currency/locale combinations
    return `${currency} ${amount.toFixed(2)}`;
  }
};

// Date formatting based on locale
export const formatDate = (
  date: Date | string | number,
  options?: Intl.DateTimeFormatOptions,
  locale?: string
): string => {
  const { i18n } = useTranslation();
  const targetLocale = locale || i18n.language;
  
  const dateObj = typeof date === 'object' ? date : new Date(date);
  
  const defaultOptions: Intl.DateTimeFormatOptions = {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  };
  
  try {
    return new Intl.DateTimeFormat(targetLocale, options || defaultOptions).format(dateObj);
  } catch (error) {
    // Fallback to basic formatting
    return dateObj.toLocaleDateString(targetLocale, defaultOptions);
  }
};

// Relative date formatting (Today, Yesterday, etc.)
export const formatRelativeDate = (
  date: Date | string | number,
  locale?: string
): string => {
  const { t } = useTranslation();
  const targetLocale = locale || useTranslation().i18n.language;
  
  const dateObj = typeof date === 'object' ? date : new Date(date);
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  const tomorrow = new Date(today);
  tomorrow.setDate(tomorrow.getDate() + 1);
  
  const inputDate = new Date(dateObj.getFullYear(), dateObj.getMonth(), dateObj.getDate());
  
  if (inputDate.getTime() === today.getTime()) {
    return t('dates.today');
  } else if (inputDate.getTime() === yesterday.getTime()) {
    return t('dates.yesterday');
  } else if (inputDate.getTime() === tomorrow.getTime()) {
    return t('dates.tomorrow');
  } else {
    return formatDate(date, { month: 'short', day: 'numeric' }, targetLocale);
  }
};

// Number formatting with thousands separators
export const formatNumber = (
  number: number,
  locale?: string
): string => {
  const { i18n } = useTranslation();
  const targetLocale = locale || i18n.language;
  
  try {
    return new Intl.NumberFormat(targetLocale).format(number);
  } catch (error) {
    // Fallback
    return number.toLocaleString(targetLocale);
  }
};

// Percentage formatting
export const formatPercentage = (
  value: number,
  locale?: string
): string => {
  const { i18n } = useTranslation();
  const targetLocale = locale || i18n.language;
  
  try {
    return new Intl.NumberFormat(targetLocale, {
      style: 'percent',
      minimumFractionDigits: 0,
      maximumFractionDigits: 2,
    }).format(value / 100);
  } catch (error) {
    // Fallback
    return `${value}%`;
  }
};

// Transaction amount formatting with proper sign
export const formatTransactionAmount = (
  amount: number,
  currency: string = 'USD',
  isOutgoing: boolean = false,
  locale?: string
): string => {
  const formattedAmount = formatCurrency(Math.abs(amount), currency, locale);
  return isOutgoing ? `-${formattedAmount}` : `+${formattedAmount}`;
};

// Utility to get currency symbol for display
export const getCurrencySymbol = (currency: string): string => {
  try {
    return (0).toLocaleString('en-US', {
      style: 'currency',
      currency: currency,
      minimumFractionDigits: 0,
      maximumFractionDigits: 0,
    }).replace(/\d/g, '').trim();
  } catch (error) {
    // Fallback to currency code
    return currency;
  }
};

// Format large numbers with abbreviations (K, M, B)
export const formatLargeNumber = (
  number: number,
  locale?: string
): string => {
  const { i18n } = useTranslation();
  const targetLocale = locale || i18n.language;
  
  if (number >= 1000000000) {
    return formatNumber(number / 1000000000, targetLocale) + 'B';
  } else if (number >= 1000000) {
    return formatNumber(number / 1000000, targetLocale) + 'M';
  } else if (number >= 1000) {
    return formatNumber(number / 1000, targetLocale) + 'K';
  }
  
  return formatNumber(number, targetLocale);
};

// Format duration in seconds to human readable format
export const formatDuration = (seconds: number, locale?: string): string => {
  const { t } = useTranslation();
  const targetLocale = locale || useTranslation().i18n.language;
  
  if (seconds < 60) {
    return `${seconds} ${t('dates.seconds', { count: seconds })}`;
  } else if (seconds < 3600) {
    const minutes = Math.floor(seconds / 60);
    return `${minutes} ${t('dates.minutes', { count: minutes })}`;
  } else if (seconds < 86400) {
    const hours = Math.floor(seconds / 3600);
    return `${hours} ${t('dates.hours', { count: hours })}`;
  } else {
    const days = Math.floor(seconds / 86400);
    return `${days} ${t('dates.days', { count: days })}`;
  }
};
