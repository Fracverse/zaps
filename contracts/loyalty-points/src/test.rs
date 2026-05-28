#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};

fn make_token(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone()).address()
}
fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}
fn tok_bal(env: &Env, token: &Address, who: &Address) -> i128 {
    soroban_sdk::token::Client::new(env, token).balance(who)
}

struct S { env: Env, client: LoyaltyPointsClient<'static>, admin: Address, token: Address }
impl S {
    fn new() -> Self {
        let env = Env::default(); env.mock_all_auths();
        let admin = Address::generate(&env);
        let token = make_token(&env, &admin);
        let id = env.register_contract(None, LoyaltyPoints);
        let client = LoyaltyPointsClient::new(&env, &id);
        client.initialize(&admin, &token, &10, &1, &0);
        let client: LoyaltyPointsClient<'static> = unsafe { core::mem::transmute(client) };
        S { env, client, admin, token }
    }
    fn fund(&self, n: i128) { mint(&self.env, &self.token, &self.admin, n); self.client.fund_reserve(&self.admin, &n); }
}

#[test] fn test_init() { let s = S::new(); assert_eq!(s.client.get_earn_rate(), 10); }
#[test] fn test_init_twice_fails() { let s = S::new(); assert_eq!(s.client.try_initialize(&s.admin, &s.token, &10, &1, &0), Err(Ok(Error::AlreadyInitialized))); }
#[test] fn test_earn() { let s = S::new(); let u = Address::generate(&s.env); assert_eq!(s.client.earn(&u, &1_000), 10); assert_eq!(s.client.balance_of(&u).points, 10); }
#[test] fn test_earn_accumulates() { let s = S::new(); let u = Address::generate(&s.env); s.client.earn(&u, &1_000); s.client.earn(&u, &2_000); assert_eq!(s.client.balance_of(&u).points, 30); }
#[test] fn test_earn_zero_fails() { let s = S::new(); assert_eq!(s.client.try_earn(&Address::generate(&s.env), &0), Err(Ok(Error::ZeroAmount))); }
#[test] fn test_redeem() { let s = S::new(); let u = Address::generate(&s.env); s.fund(50); s.client.earn(&u, &5_000); s.client.redeem(&u, &10); assert_eq!(tok_bal(&s.env, &s.token, &u), 10); assert_eq!(s.client.balance_of(&u).points, 40); }
#[test] fn test_redeem_insufficient_fails() { let s = S::new(); let u = Address::generate(&s.env); s.fund(10); s.client.earn(&u, &1_000); assert_eq!(s.client.try_redeem(&u, &50), Err(Ok(Error::InsufficientPoints))); }
#[test] fn test_points_expire() {
    let env = Env::default(); env.mock_all_auths();
    let admin = Address::generate(&env); let token = make_token(&env, &admin);
    let id = env.register_contract(None, LoyaltyPoints);
    let client = LoyaltyPointsClient::new(&env, &id);
    client.initialize(&admin, &token, &10, &1, &5);
    let u = Address::generate(&env); client.earn(&u, &1_000);
    env.ledger().with_mut(|l| l.sequence_number = 10);
    assert_eq!(client.try_redeem(&u, &5), Err(Ok(Error::PointsExpired)));
}
#[test] fn test_set_earn_rate() { let s = S::new(); s.client.set_earn_rate(&20); assert_eq!(s.client.get_earn_rate(), 20); }
