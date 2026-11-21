use super::*;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{symbol_short, Env, IntoVal};
use zaps_circle::{CircleContract, CircleContractClient};

struct Ctx {
    env: Env,
    factory: CircleFactoryClient<'static>,
    circle: CircleContractClient<'static>,
    factory_address: Address,
    circle_address: Address,
    admin: Address,
    creator: Address,
    token: Address,
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let circle_address = env.register_contract(None, CircleContract);
    let circle = CircleContractClient::new(&env, &circle_address);
    circle.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    StellarAssetClient::new(&env, &token).mint(&creator, &100);
    circle.add_supported_asset(&admin, &token);

    let factory_address = env.register_contract(None, CircleFactory);
    let factory = CircleFactoryClient::new(&env, &factory_address);
    factory.initialize(&admin, &circle_address);
    Ctx {
        env,
        factory: unsafe { core::mem::transmute(factory) },
        circle: unsafe { core::mem::transmute(circle) },
        factory_address,
        circle_address,
        admin,
        creator,
        token,
    }
}

#[test]
fn factory_creates_unique_circles_registers_them_and_emits_events() {
    let c = setup();
    let first_id = c.factory.create_circle(&c.creator, &c.token, &50);
    let second_id = c.factory.create_circle(&c.admin, &c.token, &0);

    assert_eq!((first_id, second_id), (0, 1));
    assert_eq!(c.factory.get_circle_count(), 2);

    let first = c.factory.get_circle(&first_id);
    assert_eq!(first.id, first_id);
    assert_eq!(first.circle_contract, c.circle_address);
    assert_eq!(first.circle_id, 0);
    assert_eq!(first.creator, c.creator);
    assert_eq!(first.asset, c.token);
    assert_eq!(first.max_payout, 50);
    assert_eq!(c.circle.get_circle(&first.circle_id).admin, c.creator);

    let (emitter, topics, data) = c.env.events().all().last().unwrap();
    assert_eq!(emitter, c.factory_address);
    let event_name: soroban_sdk::Symbol = topics.get(0).unwrap().into_val(&c.env);
    let emitted_id: u32 = topics.get(1).unwrap().into_val(&c.env);
    let emitted_registration: CircleRegistration = data.into_val(&c.env);
    assert_eq!(event_name, symbol_short!("circ_crt"));
    assert_eq!(emitted_id, second_id);
    assert_eq!(emitted_registration.id, second_id);
}

#[test]
fn factory_rejects_invalid_initialization_and_circle_parameters() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let circle_address = env.register_contract(None, CircleContract);
    let factory_address = env.register_contract(None, CircleFactory);
    let factory = CircleFactoryClient::new(&env, &factory_address);

    assert_eq!(
        factory.try_initialize(&admin, &factory_address),
        Err(Ok(Error::InvalidConfiguration))
    );
    let wrong_target = env.register_stellar_asset_contract(admin.clone());
    assert_eq!(
        factory.try_initialize(&admin, &wrong_target),
        Err(Ok(Error::InvalidConfiguration))
    );
    factory.initialize(&admin, &circle_address);
    assert_eq!(
        factory.try_initialize(&admin, &circle_address),
        Err(Ok(Error::AlreadyInitialized))
    );
    assert_eq!(
        factory.try_create_circle(&creator, &Address::generate(&env), &-1),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn failed_underlying_creation_is_not_registered() {
    let c = setup();
    let unsupported_asset = env_asset(&c.env, &c.admin);
    assert_eq!(
        c.factory
            .try_create_circle(&c.creator, &unsupported_asset, &0),
        Err(Ok(Error::CircleCreationFailed))
    );
    assert_eq!(c.factory.get_circle_count(), 0);
    assert_eq!(c.factory.try_get_circle(&0), Err(Ok(Error::NotFound)));
}

fn env_asset(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract(admin.clone())
}
