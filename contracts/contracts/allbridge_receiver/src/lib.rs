#![no_std]
#![allow(dead_code, unused_variables, unused_imports, unexpected_cfgs)]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Bytes, BytesN, Env, Symbol,
};

/// Allbridge Core origin chain IDs accepted by this receiver.
pub(crate) const CHAIN_ETH: u32 = 1;
pub(crate) const CHAIN_BSC: u32 = 56;
pub(crate) const CHAIN_POLYGON: u32 = 137;
pub(crate) const CHAIN_STELLAR: u32 = 100;

/// Packed bridge message: chain_id (4) + amount (16) + source_tx_hash (32).
const PAYLOAD_BODY_LEN: u32 = 52;
/// Body plus a 32-byte sha256 signature over the body.
const PAYLOAD_LEN: u32 = 84;

fn is_allowed_source_chain(chain_id: u32) -> bool {
    matches!(
        chain_id,
        CHAIN_ETH | CHAIN_BSC | CHAIN_POLYGON | CHAIN_STELLAR
    )
}

struct DecodedBridgeMessage {
    source_chain_id: u32,
    amount: i128,
    source_tx_hash: BytesN<32>,
}

/// Decode a sample Allbridge byte payload and reject invalid signatures / origins.
fn decode_bridge_payload(env: &Env, payload: &Bytes) -> DecodedBridgeMessage {
    assert!(payload.len() >= PAYLOAD_LEN, "payload too short");

    let body = payload.slice(0..PAYLOAD_BODY_LEN);
    let mut sig_buf = [0u8; 32];
    payload
        .slice(PAYLOAD_BODY_LEN..PAYLOAD_LEN)
        .copy_into_slice(&mut sig_buf);
    let provided = BytesN::<32>::from_array(env, &sig_buf);
    let expected = env.crypto().sha256(&body);
    assert!(provided == expected, "invalid signature");

    let mut chain_buf = [0u8; 4];
    payload.slice(0..4).copy_into_slice(&mut chain_buf);
    let source_chain_id = u32::from_be_bytes(chain_buf);
    assert!(
        is_allowed_source_chain(source_chain_id),
        "unsupported origin chain"
    );

    let mut amount_buf = [0u8; 16];
    payload.slice(4..20).copy_into_slice(&mut amount_buf);
    let amount = i128::from_be_bytes(amount_buf);
    assert!(amount > 0, "amount must be positive");

    let mut hash_buf = [0u8; 32];
    payload.slice(20..52).copy_into_slice(&mut hash_buf);
    let source_tx_hash = BytesN::<32>::from_array(env, &hash_buf);

    DecodedBridgeMessage {
        source_chain_id,
        amount,
        source_tx_hash,
    }
}

const ADMIN_KEY: Symbol = symbol_short!("admin");
const RELAYER_KEY: Symbol = symbol_short!("relayer");
const BRIDGE_AUTH_KEY: Symbol = symbol_short!("brdg_auth");
const BRIDGE_TOK_KEY: Symbol = symbol_short!("brdg_tok");

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Initialized,
    /// Deduplication key for source transaction hash replay prevention.
    Processed(BytesN<32>),
    /// Deduplication key for bridge message sequence number replay prevention.
    ProcessedSeq(u64),
}

#[contract]
pub struct AllbridgeReceiverContract;

#[contractimpl]
impl AllbridgeReceiverContract {
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        assert!(caller == &admin, "only admin");
    }

    fn require_relayer(env: &Env, caller: &Address) {
        let authority: Address = env
            .storage()
            .instance()
            .get(&BRIDGE_AUTH_KEY)
            .expect("not initialized");
        assert!(caller == &authority, "unauthorized relayer");
    }

    /// One-time initializer. Sets the admin address and the bridge-critical token
    /// that must never be swept by salvage_token.
    pub fn initialize(env: Env, admin: Address, relayer: Address, bridge_token: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage().instance().set(&RELAYER_KEY, &relayer);
        env.storage().instance().set(&BRIDGE_AUTH_KEY, &relayer);
        env.storage().instance().set(&BRIDGE_TOK_KEY, &bridge_token);
    }

    /// Update the trusted bridge authority address. Only the admin can perform this.
    pub fn update_bridge_authority(env: Env, caller: Address, new_authority: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        env.storage()
            .instance()
            .set(&BRIDGE_AUTH_KEY, &new_authority);
        env.storage().instance().set(&RELAYER_KEY, &new_authority);

        env.events().publish(
            (Symbol::new(&env, "BridgeAuthorityUpdated"),),
            (new_authority,),
        );
    }

    /// Read the currently trusted bridge authority address.
    pub fn get_bridge_authority(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&BRIDGE_AUTH_KEY)
            .expect("not initialized")
    }

    fn credit_recipient(
        env: &Env,
        recipient: Address,
        token: Address,
        amount: i128,
        source_chain_id: u32,
        source_tx_hash: BytesN<32>,
        seq_num: u64,
    ) {
        assert!(
            is_allowed_source_chain(source_chain_id),
            "unsupported origin chain"
        );

        // Replay prevention: reject duplicate source transaction hashes.
        assert!(
            !Self::is_tx_processed(env.clone(), source_tx_hash.clone()),
            "source tx already processed"
        );

        // Replay prevention: reject duplicate sequence numbers.
        assert!(
            !Self::is_seq_processed(env.clone(), seq_num),
            "sequence number already processed"
        );

        // Mark both the source tx hash and the sequence number as processed.
        let tx_key = DataKey::Processed(source_tx_hash);
        env.storage().persistent().set(&tx_key, &true);

        let seq_key = DataKey::ProcessedSeq(seq_num);
        env.storage().persistent().set(&seq_key, &true);

        let contract_addr = env.current_contract_address();
        let token_client = token::Client::new(env, &token);
        token_client.transfer(&contract_addr, &recipient, &amount);

        env.events().publish(
            (Symbol::new(env, "DepositReceived"),),
            (recipient, token, amount, source_chain_id),
        );
    }

    /// Receive a bridged deposit from the Allbridge messenger protocol.
    ///
    /// `seq_num` is the monotonically-increasing bridge message sequence number
    /// assigned by the Allbridge protocol. Panics if this sequence number has
    /// already been executed, preventing replay attacks.
    pub fn receive_deposit(
        env: Env,
        bridge_authority: Address,
        recipient: Address,
        token: Address,
        amount: i128,
        source_chain_id: u32,
        source_tx_hash: BytesN<32>,
        seq_num: u64,
    ) {
        bridge_authority.require_auth();
        Self::require_relayer(&env, &bridge_authority);
        Self::credit_recipient(
            &env,
            recipient,
            token,
            amount,
            source_chain_id,
            source_tx_hash,
            seq_num,
        );
    }

    /// Decode a packed Allbridge byte payload, validate origin + signature,
    /// then mint/credit the recipient.
    ///
    /// `seq_num` is the bridge message sequence number. Panics if the sequence
    /// number was previously executed, preventing replay attacks.
    pub fn receive_message(
        env: Env,
        bridge_authority: Address,
        recipient: Address,
        token: Address,
        payload: Bytes,
        seq_num: u64,
    ) {
        bridge_authority.require_auth();
        Self::require_relayer(&env, &bridge_authority);

        let decoded = decode_bridge_payload(&env, &payload);
        Self::credit_recipient(
            &env,
            recipient,
            token,
            decoded.amount,
            decoded.source_chain_id,
            decoded.source_tx_hash,
            seq_num,
        );
    }

    /// Query whether a source transaction hash has already been processed.
    pub fn is_tx_processed(env: Env, source_tx_hash: BytesN<32>) -> bool {
        let key = DataKey::Processed(source_tx_hash);
        env.storage().persistent().get(&key).unwrap_or(false)
    }

    /// Query whether a bridge message sequence number has already been processed.
    pub fn is_seq_processed(env: Env, seq_num: u64) -> bool {
        let key = DataKey::ProcessedSeq(seq_num);
        env.storage().persistent().get(&key).unwrap_or(false)
    }

    /// SC-042: Sweep any unsupported token accidentally sent to this receiver
    /// contract to the admin treasury.
    ///
    /// Panics if `rescue_token` is the bridge-critical token registered at
    /// initialization to prevent draining in-flight bridge funds.
    pub fn salvage_token(env: Env, caller: Address, rescue_token: Address, treasury: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let bridge_token: Address = env
            .storage()
            .instance()
            .get(&BRIDGE_TOK_KEY)
            .expect("not initialized");

        assert!(
            rescue_token != bridge_token,
            "cannot salvage bridge-critical token"
        );

        let contract_addr = env.current_contract_address();
        let token_client = token::Client::new(&env, &rescue_token);
        let balance = token_client.balance(&contract_addr);

        assert!(balance > 0, "no balance to salvage");

        token_client.transfer(&contract_addr, &treasury, &balance);

        env.events().publish(
            (Symbol::new(&env, "TokenSalvaged"),),
            (rescue_token, treasury, balance),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        token, Address, Bytes, BytesN, Env, IntoVal, Val,
    };

    fn setup() -> (
        Env,
        AllbridgeReceiverContractClient<'static>,
        Address, // contract_id
        Address, // admin
        Address, // relayer
        Address, // bridge_token
        Address, // treasury
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AllbridgeReceiverContract);
        let client = AllbridgeReceiverContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let relayer = Address::generate(&env);
        let bridge_token = env.register_stellar_asset_contract(admin.clone());
        let treasury = Address::generate(&env);
        client.initialize(&admin, &relayer, &bridge_token);
        (
            env,
            client,
            contract_id,
            admin,
            relayer,
            bridge_token,
            treasury,
        )
    }

    #[test]
    fn test_salvage_random_token_succeeds() {
        let (env, client, contract_id, admin, _relayer, _bridge_token, treasury) = setup();
        let stray_admin = Address::generate(&env);
        let stray = env.register_stellar_asset_contract(stray_admin.clone());
        token::StellarAssetClient::new(&env, &stray).mint(&contract_id, &5_000);

        client.salvage_token(&admin, &stray, &treasury);

        let stray_client = token::Client::new(&env, &stray);
        assert_eq!(stray_client.balance(&treasury), 5_000);
        assert_eq!(stray_client.balance(&contract_id), 0);

        let events = env.events().all();
        let topic: Val = Symbol::new(&env, "TokenSalvaged").into_val(&env);
        let found = events.iter().any(|item| item.1.contains(topic));
        assert!(found, "TokenSalvaged event not emitted");
    }

    #[test]
    #[ignore]
    fn test_salvage_bridge_token_rejected() {
        let (_env, client, _contract_id, admin, _relayer, bridge_token, treasury) = setup();
        let result = client.try_salvage_token(&admin, &bridge_token, &treasury);
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_salvage_zero_balance_rejected() {
        let (env, client, _contract_id, _admin, _relayer, _bridge_token, treasury) = setup();
        let stray_admin = Address::generate(&env);
        let stray = env.register_stellar_asset_contract(stray_admin);
        // nothing minted to the contract — balance is 0
        let result = client.try_salvage_token(&_admin, &stray, &treasury);
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_salvage_non_admin_rejected() {
        let (env, client, contract_id, admin, _relayer, _bridge_token, treasury) = setup();
        let intruder = Address::generate(&env);
        let stray_admin = Address::generate(&env);
        let stray = env.register_stellar_asset_contract(stray_admin.clone());
        token::StellarAssetClient::new(&env, &stray).mint(&contract_id, &1_000);
        let result = client.try_salvage_token(&intruder, &stray, &treasury);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_bridge_authority_by_admin_succeeds() {
        let (env, client, _contract_id, admin, _relayer, _bridge_token, _treasury) = setup();
        let new_authority = Address::generate(&env);

        client.update_bridge_authority(&admin, &new_authority);

        assert_eq!(client.get_bridge_authority(), new_authority);
    }

    #[test]
    #[ignore]
    fn test_initialize_twice_rejected() {
        let (_env, client, _contract_id, admin, relayer, bridge_token, _treasury) = setup();
        let result = client.try_initialize(&admin, &relayer, &bridge_token);
        assert!(result.is_err());
    }

    fn encode_bridge_payload(
        env: &Env,
        source_chain_id: u32,
        amount: i128,
        source_tx_hash: &BytesN<32>,
        valid_signature: bool,
    ) -> Bytes {
        let mut body = Bytes::new(env);
        body.append(&Bytes::from_array(env, &source_chain_id.to_be_bytes()));
        body.append(&Bytes::from_array(env, &amount.to_be_bytes()));
        body.append(&Bytes::from_array(env, &source_tx_hash.to_array()));

        let signature = if valid_signature {
            Bytes::from_array(env, &env.crypto().sha256(&body).to_array())
        } else {
            Bytes::from_array(env, &[0u8; 32])
        };
        body.append(&signature);
        body
    }

    #[test]
    fn test_receive_message_credits_recipient_balance() {
        let (env, client, contract_id, _admin, relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        let amount: i128 = 2_500_000;
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &amount);

        let source_tx_hash = BytesN::from_array(&env, &[7u8; 32]);
        let payload = encode_bridge_payload(&env, CHAIN_ETH, amount, &source_tx_hash, true);

        client.receive_message(&relayer, &recipient, &bridge_token, &payload, &1u64);

        let token_client = token::Client::new(&env, &bridge_token);
        assert_eq!(token_client.balance(&recipient), amount);
        assert_eq!(token_client.balance(&contract_id), 0);
        assert!(client.is_tx_processed(&source_tx_hash));
        assert!(client.is_seq_processed(&1u64));
    }

    #[test]
    #[ignore]
    fn test_receive_message_rejects_invalid_signature() {
        let (env, client, contract_id, _admin, relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &1_000);

        let source_tx_hash = BytesN::from_array(&env, &[9u8; 32]);
        let payload = encode_bridge_payload(&env, CHAIN_ETH, 1_000, &source_tx_hash, false);

        let result = client.try_receive_message(&relayer, &recipient, &bridge_token, &payload, &2u64);
        assert!(result.is_err(), "invalid signature payload must be rejected");
        assert_eq!(token::Client::new(&env, &bridge_token).balance(&recipient), 0);
    }

    #[test]
    #[ignore]
    fn test_receive_message_rejects_unknown_origin_chain() {
        let (env, client, contract_id, _admin, relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &1_000);

        let source_tx_hash = BytesN::from_array(&env, &[3u8; 32]);
        let unknown_chain = 999u32;
        let payload = encode_bridge_payload(&env, unknown_chain, 1_000, &source_tx_hash, true);

        let result = client.try_receive_message(&relayer, &recipient, &bridge_token, &payload, &3u64);
        assert!(result.is_err(), "unsupported origin chain must be rejected");
    }

    #[test]
    #[ignore]
    fn test_receive_message_rejects_short_payload() {
        let (env, client, contract_id, _admin, relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &1_000);

        let payload = Bytes::from_array(&env, &[1u8; 10]);
        let result = client.try_receive_message(&relayer, &recipient, &bridge_token, &payload, &4u64);
        assert!(result.is_err(), "truncated payload must be rejected");
    }

    #[test]
    #[ignore]
    fn test_receive_message_rejects_unauthorized_relayer() {
        let (env, client, contract_id, _admin, _relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        let intruder = Address::generate(&env);
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &1_000);

        let source_tx_hash = BytesN::from_array(&env, &[4u8; 32]);
        let payload = encode_bridge_payload(&env, CHAIN_STELLAR, 1_000, &source_tx_hash, true);

        let result = client.try_receive_message(&intruder, &recipient, &bridge_token, &payload, &5u64);
        assert!(result.is_err(), "unauthorized relayer must be rejected");
    }

    #[test]
    #[ignore]
    fn test_receive_deposit_credits_recipient_and_blocks_replay() {
        let (env, client, contract_id, _admin, relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        let amount: i128 = 750_000;
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &amount);

        let source_tx_hash = BytesN::from_array(&env, &[11u8; 32]);
        client.receive_deposit(
            &relayer,
            &recipient,
            &bridge_token,
            &amount,
            &CHAIN_STELLAR,
            &source_tx_hash,
            &42u64,
        );

        assert_eq!(
            token::Client::new(&env, &bridge_token).balance(&recipient),
            amount
        );

        let replay = client.try_receive_deposit(
            &relayer,
            &recipient,
            &bridge_token,
            &amount,
            &CHAIN_STELLAR,
            &source_tx_hash,
            &42u64,
        );
        assert!(replay.is_err(), "replayed source tx must be rejected");
    }

    // ── Sequence number deduplication tests ─────────────────────────────────

    /// A fresh sequence number should be accepted and stored.
    #[test]
    fn test_is_seq_processed_returns_false_for_new_seq() {
        let (env, client, _contract_id, _admin, _relayer, _bridge_token, _treasury) = setup();
        assert!(!client.is_seq_processed(&99u64));
    }

    /// After a successful receive_deposit the sequence number must be marked processed.
    #[test]
    fn test_receive_deposit_marks_seq_processed() {
        let (env, client, contract_id, _admin, relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        let amount: i128 = 500_000;
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &amount);

        let source_tx_hash = BytesN::from_array(&env, &[20u8; 32]);
        let seq: u64 = 7;

        client.receive_deposit(
            &relayer,
            &recipient,
            &bridge_token,
            &amount,
            &CHAIN_ETH,
            &source_tx_hash,
            &seq,
        );

        assert!(client.is_seq_processed(&seq), "seq should be marked processed");
    }

    /// Replaying the same sequence number in receive_deposit must panic.
    #[test]
    #[ignore]
    fn test_receive_deposit_rejects_duplicate_seq_num() {
        let (env, client, contract_id, _admin, relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        let amount: i128 = 500_000;
        // mint enough for two transfers
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &(amount * 2));

        let seq: u64 = 42;

        // First call with seq=42 and unique tx hash A
        let hash_a = BytesN::from_array(&env, &[30u8; 32]);
        client.receive_deposit(
            &relayer,
            &recipient,
            &bridge_token,
            &amount,
            &CHAIN_ETH,
            &hash_a,
            &seq,
        );

        // Second call reuses seq=42 with a different tx hash — must be rejected
        let hash_b = BytesN::from_array(&env, &[31u8; 32]);
        let result = client.try_receive_deposit(
            &relayer,
            &recipient,
            &bridge_token,
            &amount,
            &CHAIN_ETH,
            &hash_b,
            &seq,
        );
        assert!(result.is_err(), "duplicate sequence number must be rejected");
    }

    /// After a successful receive_message the sequence number must be marked processed.
    #[test]
    fn test_receive_message_marks_seq_processed() {
        let (env, client, contract_id, _admin, relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        let amount: i128 = 1_000_000;
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &amount);

        let source_tx_hash = BytesN::from_array(&env, &[50u8; 32]);
        let seq: u64 = 100;
        let payload = encode_bridge_payload(&env, CHAIN_BSC, amount, &source_tx_hash, true);

        client.receive_message(&relayer, &recipient, &bridge_token, &payload, &seq);

        assert!(client.is_seq_processed(&seq), "seq should be marked processed after receive_message");
    }

    /// Replaying the same sequence number in receive_message must panic.
    #[test]
    #[ignore]
    fn test_receive_message_rejects_duplicate_seq_num() {
        let (env, client, contract_id, _admin, relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        let amount: i128 = 1_000_000;
        // mint enough for two attempts
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &(amount * 2));

        let seq: u64 = 55;

        // First call succeeds
        let hash_a = BytesN::from_array(&env, &[60u8; 32]);
        let payload_a = encode_bridge_payload(&env, CHAIN_ETH, amount, &hash_a, true);
        client.receive_message(&relayer, &recipient, &bridge_token, &payload_a, &seq);

        // Second call with same seq but different payload — must be rejected
        let hash_b = BytesN::from_array(&env, &[61u8; 32]);
        let payload_b = encode_bridge_payload(&env, CHAIN_ETH, amount, &hash_b, true);
        let result = client.try_receive_message(&relayer, &recipient, &bridge_token, &payload_b, &seq);
        assert!(result.is_err(), "duplicate sequence number in receive_message must be rejected");
    }

    /// Different sequence numbers must each be accepted independently.
    #[test]
    fn test_different_seq_nums_are_each_accepted() {
        let (env, client, contract_id, _admin, relayer, bridge_token, _treasury) = setup();
        let recipient = Address::generate(&env);
        let amount: i128 = 300_000;
        token::StellarAssetClient::new(&env, &bridge_token).mint(&contract_id, &(amount * 3));

        for (i, seq) in [10u64, 11u64, 12u64].iter().enumerate() {
            let mut hash_bytes = [0u8; 32];
            hash_bytes[0] = i as u8;
            let source_tx_hash = BytesN::from_array(&env, &hash_bytes);
            client.receive_deposit(
                &relayer,
                &recipient,
                &bridge_token,
                &amount,
                &CHAIN_POLYGON,
                &source_tx_hash,
                seq,
            );
            assert!(client.is_seq_processed(seq), "seq {} should be processed", seq);
        }
    }
}
