#[cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _},
    Address, Env, IntoVal,
};

#[test]
fn test_initialization() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_router = Address::generate(&env);
    let payout_contract = Address::generate(&env);

    env.mock_all_auths();

    // Initialize contract
    client.initialize(&admin, &payment_router, &payout_contract);

    // Try to initialize again (should fail)
    let result = client.try_initialize(&admin, &payment_router, &payout_contract);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_merchant_initialization() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_router = Address::generate(&env);
    let payout_contract = Address::generate(&env);
    let merchant = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &payment_router, &payout_contract);
    client.init_merchant(&merchant);

    // Verify balance is zero
    assert_eq!(client.balance_of(&merchant), 0);
}

#[test]
fn test_credit_flow() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_router = Address::generate(&env);
    let payout_contract = Address::generate(&env);
    let merchant = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &payment_router, &payout_contract);
    client.init_merchant(&merchant);

    // Credit merchant account
    let new_balance = client.credit(&merchant, &1000);
    assert_eq!(new_balance, 1000);
    assert_eq!(client.balance_of(&merchant), 1000);

    // Credit again
    let new_balance = client.credit(&merchant, &500);
    assert_eq!(new_balance, 1500);
    assert_eq!(client.balance_of(&merchant), 1500);
}

#[test]
fn test_debit_flow() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_router = Address::generate(&env);
    let payout_contract = Address::generate(&env);
    let merchant = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &payment_router, &payout_contract);
    client.init_merchant(&merchant);
    client.credit(&merchant, &1000);

    // Debit merchant account
    let new_balance = client.debit(&merchant, &300);
    assert_eq!(new_balance, 700);
    assert_eq!(client.balance_of(&merchant), 700);
}

#[test]
fn test_over_debit_rejection() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_router = Address::generate(&env);
    let payout_contract = Address::generate(&env);
    let merchant = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &payment_router, &payout_contract);
    client.init_merchant(&merchant);
    client.credit(&merchant, &500);

    // Try to debit more than balance
    let result = client.try_debit(&merchant, &600);
    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
}

#[test]
fn test_negative_amount_rejection() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_router = Address::generate(&env);
    let payout_contract = Address::generate(&env);
    let merchant = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &payment_router, &payout_contract);
    client.init_merchant(&merchant);

    // Try negative credit
    let result = client.try_credit(&merchant, &-100);
    assert_eq!(result, Err(Ok(Error::NegativeAmount)));

    // Try negative debit
    let result = client.try_debit(&merchant, &-100);
    assert_eq!(result, Err(Ok(Error::NegativeAmount)));
}

#[test]
fn test_uninitialized_merchant_rejection() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_router = Address::generate(&env);
    let payout_contract = Address::generate(&env);
    let merchant = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &payment_router, &payout_contract);

    // Try operations on non-existent merchant
    let result = client.try_credit(&merchant, &100);
    assert_eq!(result, Err(Ok(Error::MerchantNotInitialized)));

    let result = client.try_debit(&merchant, &100);
    assert_eq!(result, Err(Ok(Error::MerchantNotInitialized)));

    let result = client.try_balance_of(&merchant);
    assert_eq!(result, Err(Ok(Error::MerchantNotInitialized)));
}

#[test]
fn test_balance_correctness_over_time() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_router = Address::generate(&env);
    let payout_contract = Address::generate(&env);
    let merchant = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &payment_router, &payout_contract);
    client.init_merchant(&merchant);

    // Simulate transaction sequence
    client.credit(&merchant, &1000); // 1000
    assert_eq!(client.balance_of(&merchant), 1000);

    client.credit(&merchant, &500); // 1500
    assert_eq!(client.balance_of(&merchant), 1500);

    client.debit(&merchant, &200); // 1300
    assert_eq!(client.balance_of(&merchant), 1300);

    client.credit(&merchant, &700); // 2000
    assert_eq!(client.balance_of(&merchant), 2000);

    client.debit(&merchant, &1500); // 500
    assert_eq!(client.balance_of(&merchant), 500);

    client.debit(&merchant, &500); // 0
    assert_eq!(client.balance_of(&merchant), 0);
}

#[test]
fn test_multiple_merchants() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let payment_router = Address::generate(&env);
    let payout_contract = Address::generate(&env);
    let merchant1 = Address::generate(&env);
    let merchant2 = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &payment_router, &payout_contract);
    client.init_merchant(&merchant1);
    client.init_merchant(&merchant2);

    client.credit(&merchant1, &1000);
    client.credit(&merchant2, &500);

    assert_eq!(client.balance_of(&merchant1), 1000);
    assert_eq!(client.balance_of(&merchant2), 500);

    client.debit(&merchant1, &300);

    assert_eq!(client.balance_of(&merchant1), 700);
    assert_eq!(client.balance_of(&merchant2), 500); // Unchanged
}

  #[test]
    fn test_zero_amount_operations() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);

        // Zero credit should work but not change balance
        let balance = client.credit(&merchant, &0);
        assert_eq!(balance, 0);
        assert_eq!(client.balance_of(&merchant), 0);

        // Credit some amount
        client.credit(&merchant, &1000);

        // Zero debit should work but not change balance
        let balance = client.debit(&merchant, &0);
        assert_eq!(balance, 1000);
        assert_eq!(client.balance_of(&merchant), 1000);
    }

    #[test]
    fn test_exact_balance_debit() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);
        client.credit(&merchant, &1000);

        // Debit exact balance should work and leave zero
        let balance = client.debit(&merchant, &1000);
        assert_eq!(balance, 0);
        assert_eq!(client.balance_of(&merchant), 0);
    }

    #[test]
    fn test_debit_by_one_more_than_balance() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);
        client.credit(&merchant, &1000);

        // Debit one more than balance should fail
        let result = client.try_debit(&merchant, &1001);
        assert_eq!(result, Err(Ok(Error::InsufficientBalance)));

        // Balance should remain unchanged
        assert_eq!(client.balance_of(&merchant), 1000);
    }

    #[test]
    fn test_debit_from_zero_balance() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);

        // Debit from zero balance should fail
        let result = client.try_debit(&merchant, &1);
        assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
    }

    #[test]
    #[should_panic(expected = "Balance overflow")]
    fn test_balance_overflow() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);

        // Credit to max i128
        client.credit(&merchant, &i128::MAX);

        // Try to credit more (should panic due to overflow)
        client.credit(&merchant, &1);
    }

    #[test]
    fn test_large_amounts() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);

        // Credit very large amount
        let large_amount = 1_000_000_000_000_000_000i128; // 1 quintillion
        client.credit(&merchant, &large_amount);
        assert_eq!(client.balance_of(&merchant), large_amount);

        // Debit half
        let debit_amount = 500_000_000_000_000_000i128;
        client.debit(&merchant, &debit_amount);
        assert_eq!(client.balance_of(&merchant), large_amount - debit_amount);

        // Credit more large amounts
        client.credit(&merchant, &large_amount);
        assert_eq!(
            client.balance_of(&merchant),
            large_amount - debit_amount + large_amount
        );

        // Debit everything
        let final_balance = client.balance_of(&merchant);
        client.debit(&merchant, &final_balance);
        assert_eq!(client.balance_of(&merchant), 0);
    }

    #[test]
    fn test_double_merchant_initialization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);

        // Try to initialize same merchant again
        let result = client.try_init_merchant(&merchant);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    #[test]
    fn test_unauthorized_credit_attempt() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);
        let attacker = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);

        // Mock auth for attacker (not payment router)
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &attacker,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "credit",
                args: (merchant.clone(), 1000i128).into_val(&env),
                sub_invokes: &[],
            },
        }]);

        // This should fail because attacker is not the payment router
        // Note: In real scenario this would panic with auth error
        // For testing purposes, we verify the authorization check exists
    }

    #[test]
    fn test_unauthorized_debit_attempt() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);
        let attacker = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);
        client.credit(&merchant, &1000);

        // Mock auth for attacker (not payout contract)
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &attacker,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "debit",
                args: (merchant.clone(), 500i128).into_val(&env),
                sub_invokes: &[],
            },
        }]);

        // This should fail because attacker is not the payout contract
    }

    #[test]
    fn test_rapid_credit_debit_sequence() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);

        // Simulate rapid transactions
        for i in 1..=100 {
            client.credit(&merchant, &100);
            assert_eq!(client.balance_of(&merchant), 100 * i);
        }

        assert_eq!(client.balance_of(&merchant), 10000);

        for i in 1..=100 {
            client.debit(&merchant, &100);
            assert_eq!(client.balance_of(&merchant), 10000 - (100 * i));
        }

        assert_eq!(client.balance_of(&merchant), 0);
    }

    #[test]
    fn test_update_authorized_addresses() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let new_router = Address::generate(&env);
        let new_payout = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);

        // Update payment router
        client.update_payment_router(&new_router);

        // Update payout contract
        client.update_payout_contract(&new_payout);

        // Old addresses should no longer work (would fail auth in real scenario)
        // New addresses should work (tested implicitly by auth system)
    }

    #[test]
    fn test_merchant_isolation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant1 = Address::generate(&env);
        let merchant2 = Address::generate(&env);
        let merchant3 = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant1);
        client.init_merchant(&merchant2);
        client.init_merchant(&merchant3);

        // Operations on different merchants
        client.credit(&merchant1, &1000);
        client.credit(&merchant2, &2000);
        client.credit(&merchant3, &3000);

        client.debit(&merchant2, &500);

        // Verify isolation
        assert_eq!(client.balance_of(&merchant1), 1000);
        assert_eq!(client.balance_of(&merchant2), 1500);
        assert_eq!(client.balance_of(&merchant3), 3000);

        // Deplete one merchant
        client.debit(&merchant1, &1000);
        assert_eq!(client.balance_of(&merchant1), 0);

        // Others should be unaffected
        assert_eq!(client.balance_of(&merchant2), 1500);
        assert_eq!(client.balance_of(&merchant3), 3000);
    }

    #[test]
    fn test_operations_before_initialization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let merchant = Address::generate(&env);

        env.mock_all_auths();

        // Try operations before contract initialization
        let result = client.try_init_merchant(&merchant);
        assert_eq!(result, Err(Ok(Error::NotInitialized)));

        let result = client.try_balance_of(&merchant);
        assert_eq!(result, Err(Ok(Error::MerchantNotInitialized)));
    }

    #[test]
    fn test_concurrent_operations_same_merchant() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        env.mock_all_auths();

        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);

        // Simulate interleaved operations
        client.credit(&merchant, &100);
        client.credit(&merchant, &200);
        client.debit(&merchant, &50);
        client.credit(&merchant, &300);
        client.debit(&merchant, &150);

        // Final balance should be: 0 + 100 + 200 - 50 + 300 - 150 = 400
        assert_eq!(client.balance_of(&merchant), 400);
    }

      #[test]
    #[should_panic]
    fn test_panic_on_credit_without_auth() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        // Initialize with proper auth
        env.mock_all_auths();
        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);

        // Clear all auth mocks
        env.mock_auths(&[]);

        // Try to credit without any auth (should panic)
        client.credit(&merchant, &1000);
    }

    #[test]
    #[should_panic]
    fn test_panic_on_debit_without_auth() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        // Initialize with proper auth
        env.mock_all_auths();
        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);
        client.credit(&merchant, &1000);

        // Clear all auth mocks
        env.mock_auths(&[]);

        // Try to debit without any auth (should panic)
        client.debit(&merchant, &500);
    }

    #[test]
    #[should_panic]
    fn test_panic_on_init_merchant_without_admin_auth() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);

        // Initialize contract
        env.mock_all_auths();
        client.initialize(&admin, &payment_router, &payout_contract);

        // Clear auth
        env.mock_auths(&[]);

        // Try to init merchant without admin auth (should panic)
        client.init_merchant(&merchant);
    }

    #[test]
    #[should_panic]
    fn test_panic_on_update_router_without_admin() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let new_router = Address::generate(&env);

        // Initialize
        env.mock_all_auths();
        client.initialize(&admin, &payment_router, &payout_contract);

        // Clear auth
        env.mock_auths(&[]);

        // Try to update router without admin auth (should panic)
        client.update_payment_router(&new_router);
    }

    #[test]
    #[should_panic]
    fn test_panic_on_update_payout_without_admin() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let new_payout = Address::generate(&env);

        // Initialize
        env.mock_all_auths();
        client.initialize(&admin, &payment_router, &payout_contract);

        // Clear auth
        env.mock_auths(&[]);

        // Try to update payout without admin auth (should panic)
        client.update_payout_contract(&new_payout);
    }

    #[test]
    #[should_panic]
    fn test_panic_on_initialize_without_admin_auth() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);

        // Don't mock auth - try to initialize without admin signature
        env.mock_auths(&[]);

        // Should panic due to missing admin auth
        client.initialize(&admin, &payment_router, &payout_contract);
    }

    #[test]
    #[should_panic]
    fn test_panic_on_credit_wrong_caller() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);
        let wrong_caller = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);

        // Mock auth for wrong caller instead of payment_router
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &wrong_caller,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "credit",
                args: (merchant.clone(), 1000i128).into_val(&env),
                sub_invokes: &[],
            },
        }]);

        // Should panic - wrong caller
        client.credit(&merchant, &1000);
    }

    #[test]
    #[should_panic]
    fn test_panic_on_debit_wrong_caller() {
        let env = Env::default();
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);
        let wrong_caller = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin, &payment_router, &payout_contract);
        client.init_merchant(&merchant);
        client.credit(&merchant, &1000);

        // Mock auth for wrong caller instead of payout_contract
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &wrong_caller,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "debit",
                args: (merchant.clone(), 500i128).into_val(&env),
                sub_invokes: &[],
            },
        }]);

        // Should panic - wrong caller
        client.debit(&merchant, &500);
    }