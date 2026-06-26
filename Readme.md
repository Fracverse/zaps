# Zaps: Social Payments on Stellar ⚡

Zaps is a high-speed, interactive social payments platform built on the Stellar blockchain with Soroban smart contracts. It transforms standard financial transactions into peer-to-peer social interactions—allowing users to pay friends, add comments, toggle likes, and share payments publicly or privately, similar to Venmo and Cash App.

---

## 🚀 Key Features

1. **Social Payments Feed**: Share peer-to-peer payments (e.g. *"John paid ₦5,000 to Doe for Lunch"*) with interactive liking and commenting.
2. **Fiat Settlement via Stellar Anchors**: Seamlessly deposit and withdraw fiat currencies (including Naira ₦) with automated conversion and settlement handled directly through regulated Stellar Anchors (SEP-24 / SEP-38).
3. **Cross-Chain Bridge Funding**: Fund your Stellar wallet from other major blockchains (Ethereum, Solana, BNB Chain, Polygon) using our Allbridge Core integration.
4. **Soroban Smart Contracts**: High-speed, secure, and gas-efficient execution of payments and social graphs on-chain.

---

## 📁 Repository Architecture

- **`mobileapp/`**: React Native (Expo) app for social payment interactions, profile management, and Allbridge cross-chain funding.
- **`backend/`**: Axum Rust server. Manages off-chain social logs (likes, comments, friends lists) and indices Stellar ledger events.
- **`contracts/`**: Soroban smart contracts workspace handling user registries, social payments, and graph relationships.
- **`dashboard/`**: Next.js web application for monitoring social statistics, Naira transaction volume, and bridging queues.

---

## 🛠️ Getting Started

### 📱 Mobile App (Expo)
```bash
cd mobileapp
npm install
npm start
```

### 🦀 Backend API (Rust)
```bash
cd backend
cargo run
```

### ⛓️ Smart Contracts (Soroban)
```bash
cd contracts
cargo build --target wasm32-unknown-unknown --release
cargo test
```

---


### 5. DevOps & Infrastructure (`/issues/devops/`)
- `[DO-001]` to `[DO-005]`: Docker configurations, compilation pipelines, deployment templates, and OpenAPI endpoints documentation.
