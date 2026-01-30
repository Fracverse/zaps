use soroban_sdk::{symbol_short, Address, Env, String, Symbol};

const INIT: Symbol = symbol_short!("init");
const REGISTER: Symbol = symbol_short!("register");

pub fn emit_initialized(env: &Env, admin: &Address) {
    env.events().publish((INIT,), admin.clone());
}

pub fn emit_registered(env: &Env, name: &String, address: &Address) {
    env.events()
        .publish((REGISTER, name.clone()), address.clone());
}
