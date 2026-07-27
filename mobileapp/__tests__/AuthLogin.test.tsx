import React, { useEffect } from 'react';
import { render, waitFor } from '@testing-library/react-native';
import { usePrivy } from '@privy-io/expo';
import { useRouter } from 'expo-router';
import { View } from 'react-native';

// Mock component simulating the Auth guard routing logic
const MockAuthGuard = () => {
  const { user, isReady } = usePrivy();
  const router = useRouter();

  useEffect(() => {
    if (isReady && user) {
      // Assuming user has no username/profile, redirect to username setup screen
      router.push('/username');
    }
  }, [user, isReady, router]);

  return <View testID="auth-guard" />;
};

jest.mock('expo-router', () => ({
  useRouter: jest.fn(() => ({
    push: jest.fn(),
    replace: jest.fn(),
  })),
}));

describe('Login Routing Behavior', () => {
  afterEach(() => {
    jest.clearAllMocks();
  });

  it('redirects to onboarding username screen when user is authenticated but needs setup', async () => {
    const mockPush = jest.fn();
    (useRouter as jest.Mock).mockReturnValue({ push: mockPush });

    // Mock Privy returning an authenticated user
    (usePrivy as jest.Mock).mockReturnValue({
      isReady: true,
      user: { id: 'did:privy:123' },
    });

    render(<MockAuthGuard />);

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith('/username');
    });
  });

  it('does not redirect if privy is not ready', () => {
    const mockPush = jest.fn();
    (useRouter as jest.Mock).mockReturnValue({ push: mockPush });

    // Mock Privy returning unready state
    (usePrivy as jest.Mock).mockReturnValue({
      isReady: false,
      user: null,
    });

    render(<MockAuthGuard />);
    
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('does not redirect if there is no authenticated user', () => {
    const mockPush = jest.fn();
    (useRouter as jest.Mock).mockReturnValue({ push: mockPush });

    // Mock Privy returning ready but unauthenticated state
    (usePrivy as jest.Mock).mockReturnValue({
      isReady: true,
      user: null,
    });

    render(<MockAuthGuard />);
    
    expect(mockPush).not.toHaveBeenCalled();
  });
});
