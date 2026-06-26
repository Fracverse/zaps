import os
import subprocess
import json

# Setup directories
BASE_DIR = "/Users/mac/Desktop/codes/drips_network/zaps"
ISSUES_DIR = os.path.join(BASE_DIR, "issues")

CATEGORIES = {
    "SC": "smart-contract",
    "BE": "backend",
    "FE": "mobile-app",
    "DB": "dashboard",
    "DO": "devops"
}

# Sub-directories
subdirs = {
    "SC": "smart-contracts",
    "BE": "backend",
    "FE": "mobile-app",
    "DB": "dashboard",
    "DO": "devops"
}

for sd in subdirs.values():
    os.makedirs(os.path.join(ISSUES_DIR, sd), exist_ok=True)

# Define template
ISSUE_TEMPLATE = """# {title}

## Description
{description}

## Files to Edit/Create
{files}

## Acceptance Criteria
{criteria}

## Guidance / Hints
{guid}
"""

# Let's define the 60 issues
issues_data = []

# --- SMART CONTRACTS (SC-001 to SC-015) ---
for i in range(1, 16):
    num = f"{i:03d}"
    if i == 1:
        title = "Implement Username Registration mapping Address -> Zaps ID"
        desc = "Implement the core registration function to allow users to register a unique Zaps username (e.g. ebube.zaps)."
        files = "- `contracts/contracts/user_registry/src/lib.rs`"
        crit = "- Ensure Address to String map is correctly written and retrieved.\n- Validate that registrations are unique."
        guid = "Use Soroban SDK `env.storage().persistent()` to store address-username mappings."
    elif i == 2:
        title = "Add validation rules for Zaps ID registration"
        desc = "Add rules to username registration: must be lowercase, alphanumeric, and between 3-15 characters long."
        files = "- `contracts/contracts/user_registry/src/lib.rs`"
        crit = "- Error if username contains capital letters or special chars.\n- Error if length < 3 or > 15."
        guid = "Iterate over characters of the String or use simple ASCII check to enforce alphanumeric."
    elif i == 3:
        title = "Implement profile avatar URI update function in Registry"
        desc = "Allow registered users to update their avatar URI in the user registry contract."
        files = "- `contracts/contracts/user_registry/src/lib.rs`"
        crit = "- Require authorization from the user's address.\n- Store the avatar URI mapping successfully."
        guid = "Call `user.require_auth()` before writing update."
    elif i == 4:
        title = "Define core Social Payment data structures"
        desc = "Define structs for Payment representing the details of social peer-to-peer payments."
        files = "- `contracts/contracts/social_payment/src/lib.rs`"
        crit = "- Struct must contain sender, receiver, token, amount, memo description, visibility enum."
        guid = "Derive `#[contracttype]` for custom structs in Soroban."
    elif i == 5:
        title = "Implement P2P payment execution function with Naira token"
        desc = "Build the core pay function transferring Naira stablecoin tokens between two users."
        files = "- `contracts/contracts/social_payment/src/lib.rs`"
        crit = "- Transfer naira tokens from sender to receiver.\n- Support custom currency checks."
        guid = "Instantiate TokenClient for the Naira token address and call `transfer`."
    elif i == 6:
        title = "Emit detailed SocialPaymentEvent on successful payment"
        desc = "Emit a rich event on payment so the background indexer can parse it and write to database."
        files = "- `contracts/contracts/social_payment/src/lib.rs`"
        crit = "- Event must contain sender, receiver, amount, memo note, and visibility tier."
        guid = "Use `env.events().publish()` to broadcast structured payment tags."
    elif i == 7:
        title = "Implement on-chain like tracking in payment contract"
        desc = "Allow users to send a 'like' transaction referencing a payment's on-chain transaction hash."
        files = "- `contracts/contracts/social_payment/src/lib.rs`"
        crit = "- Require sender auth.\n- Emit a PaymentLiked event containing tx hash and user address."
        guid = "Emit a clean event that the indexer can parse, keeping storage costs minimal."
    elif i == 8:
        title = "Implement on-chain comment event emission"
        desc = "Allow users to post comment messages targeting a payment's on-chain identifier."
        files = "- `contracts/contracts/social_payment/src/lib.rs`"
        crit = "- Limit comment string length to 120 chars.\n- Emit PaymentCommented event containing comment text."
        guid = "Validate comment string length using `comment.len()` before emitting."
    elif i == 9:
        title = "Add access control modifier to registry contract actions"
        desc = "Ensure admin actions or registration locks are guarded by appropriate authorizations."
        files = "- `contracts/contracts/user_registry/src/lib.rs`"
        crit = "- Non-authorized users should fail to execute update profiles on behalf of others."
        guid = "Implement authorization checks using `Address::require_auth()`."
    elif i == 10:
        title = "Implement Naira Token minting/burning interface for Anchors"
        desc = "Allow the verified Stellar anchor address to mint/burn Naira stablecoins (₦) on Soroban."
        files = "- `contracts/contracts/naira_token/src/lib.rs`"
        crit = "- Enforce that only the admin/anchor address can call mint and burn."
        guid = "Check administrator address matches stored admin state before execution."
    elif i == 11:
        title = "Implement Naira Token balance inquiry and allowance controls"
        desc = "Add ERC20-like allowance permissions for naira tokens to support smart contract withdrawals."
        files = "- `contracts/contracts/naira_token/src/lib.rs`"
        crit = "- Transfer checks validation.\n- Correct allowance deductions."
        guid = "Follow standard token interface designs in Soroban SDK."
    elif i == 12:
        title = "Implement social friendship graph registry on-chain"
        desc = "Track user friends lists on-chain to handle friends-only payments permission check."
        files = "- `contracts/contracts/social_graph/src/lib.rs`"
        crit = "- Actions `add_friend` and `remove_friend` must succeed with user's signatures."
        guid = "Store as persistent mapping of address pairs."
    elif i == 13:
        title = "Implement fee distribution logic on social transfers"
        desc = "Deduct a 0.1% platform fee on public payments and route it to a treasury address."
        files = "- `contracts/contracts/social_payment/src/lib.rs`"
        crit = "- Calculate fee accurately (amount * 1 / 1000).\n- Transfer fee to the designated address."
        guid = "Handle integer division rounding down safely."
    elif i == 14:
        title = "Implement contract upgradeability interface"
        desc = "Support hot upgrading contract logic without losing stored state."
        files = "- `contracts/contracts/social_payment/src/lib.rs`"
        crit = "- Enforce upgrade can only be triggered by the multisig admin address."
        guid = "Use `env.deployer().update_current_contract_wasm()`."
    elif i == 15:
        title = "Write comprehensive unit test suite for Social Payment contract"
        desc = "Create mock test ledger environment to test full pay, like, and comment flows."
        files = "- `contracts/contracts/social_payment/src/lib.rs` (plus tests module)"
        crit = "- Coverage should include public, friends-only, and private payment validations."
        guid = "Use `Env::default()` and register client contract instances."
    else:
        continue
    issues_data.append({"cat": "SC", "num": num, "title": title, "desc": desc, "files": files, "crit": crit, "guid": guid})

# --- BACKEND (BE-001 to BE-020) ---
for i in range(1, 21):
    num = f"{i:03d}"
    if i == 1:
        title = "Setup PostgreSQL database connection pool and database migration framework"
        desc = "Initialize SQLx connection pool and load migrations successfully on startup."
        files = "- `backend/src/main.rs` \n- `backend/src/db/mod.rs`"
        crit = "- Server must read DATABASE_URL and launch migrations."
        guid = "Use `sqlx::migrate!()` to run the migration directory on bootstrap."
    elif i == 2:
        title = "Create database schemas for Users, Payments, Likes, Comments, Friendships"
        desc = "Implement the tables and constraints needed to save social and financial records in PostgreSQL."
        files = "- `backend/src/db/schema.sql`"
        crit = "- Tables created with primary/foreign keys, correct constraints, index on visibility."
        guid = "Define table attributes carefully and use indexes on frequently queried search text/hashes."
    elif i == 3:
        title = "Implement Stellar wallet-based authentication challenge-response route"
        desc = "Generate cryptographically secure mock challenges for client signatures and verify them."
        files = "- `backend/src/api/auth.rs`"
        crit = "- Verify signature matches wallet address.\n- Return JWT token on successful sign-in."
        guid = "Verify messages using standard Stellar signature verification algorithms (Ed25519)."
    elif i == 4:
        title = "Implement User Profile CRUD endpoints"
        desc = "Allow users to read and update their display name, bio, and avatar URLs."
        files = "- `backend/src/api/user.rs`"
        crit = "- Updates must only affect the authenticated user's records."
        guid = "Extract the current user address from JWT authorization header."
    elif i == 5:
        title = "Implement regex-based user search endpoint"
        desc = "Support searching other users by their unique Zaps ID (alphanumeric search) for payment routing."
        files = "- `backend/src/api/user.rs`"
        crit = "- Match prefixes and return paginated user profiles."
        guid = "Use `LIKE 'query%'` in SQL queries for fast indexed lookups."
    elif i == 6:
        title = "Create Feed Fetch API endpoint for Public payments"
        desc = "Return a scrollable, paginated public feed of social payments across the platform."
        files = "- `backend/src/api/feed.rs`"
        crit = "- Exclude transactions marked private or friends-only."
        guid = "Filter in SQL: `WHERE visibility = 'PUBLIC'` order by `created_at DESC`."
    elif i == 7:
        title = "Create Feed Fetch API endpoint for Friend-only payments"
        desc = "Return a feed of social payments involving the authenticated user or their friends."
        files = "- `backend/src/api/feed.rs`"
        crit = "- Enforce friends visibility rules (check friendship table before returning records)."
        guid = "Use a SQL `JOIN` on friendships where status is 'ACCEPTED'."
    elif i == 8:
        title = "Create Feed Fetch API endpoint for Personal/Private payments"
        desc = "Return personal social payment history (Private visibility items)."
        files = "- `backend/src/api/feed.rs`"
        crit = "- Strictly check that only sender or receiver can retrieve private items."
        guid = "Enforce `WHERE (sender_id = :me OR receiver_id = :me)` in private feed queries."
    elif i == 9:
        title = "Implement Like/Unlike endpoint for payments"
        desc = "Store a user's like reaction in the database and handle toggling state."
        files = "- `backend/src/api/social.rs`"
        crit = "- Toggle like state and update like counter.\n- Avoid double likes via unique constraint checks."
        guid = "Use `INSERT INTO likes ... ON CONFLICT DO NOTHING` or handle delete on double tap."
    elif i == 10:
        title = "Implement Comment creation and deletion endpoints"
        desc = "Allow users to write/delete comment logs attached to payments."
        files = "- `backend/src/api/social.rs`"
        crit = "- Check comment owner matches authenticated user before deletion."
        guid = "Perform comment validation in Axum routes before updating db."
    elif i == 11:
        title = "Implement Friend Request send, accept, and reject routes"
        desc = "Handle user social links status logic (PENDING -> ACCEPTED / REJECTED)."
        files = "- `backend/src/api/user.rs`"
        crit = "- Update database state and prevent sending double requests."
        guid = "Insert friendship record with PENDING status. Accept changes it to ACCEPTED."
    elif i == 12:
        title = "Implement friend list retrieval API endpoint"
        desc = "Return all friends of the authenticated user to select for transfers."
        files = "- `backend/src/api/user.rs`"
        crit = "- Only return friends with ACCEPTED status."
        guid = "Perform quick index search in friendships table."
    elif i == 13:
        title = "Design and build Stellar Event Indexer worker to poll Soroban RPC"
        desc = "Poller process querying Soroban RPC for payment events on the ledger."
        files = "- `backend/src/indexer/worker.rs`"
        crit = "- Loop consistently, handle RPC disconnections, track cursor position."
        guid = "Implement backoff logic if RPC endpoint returns errors."
    elif i == 14:
        title = "Implement parser for SocialPaymentEvent ledger logs"
        desc = "Decode blockchain event payloads into fields: sender, receiver, amount, memo, visibility."
        files = "- `backend/src/indexer/worker.rs`"
        crit = "- Decrypted event fields must match database format."
        guid = "Map on-chain addresses to database user UUIDs before inserts."
    elif i == 15:
        title = "Implement indexing status tracker to prevent double-processing"
        desc = "Save latest ledger checkpoint sequence in DB to survive worker restarts."
        files = "- `backend/src/indexer/worker.rs`"
        crit = "- Store sequence on successful block scan.\n- Read checkpoint on boot."
        guid = "Write ledger sequence updates inside the transaction scope of parsed events."
    elif i == 16:
        title = "Implement Allbridge API proxy endpoint to get bridge fee quotes"
        desc = "Provide client route fetching swap fee schedules from Allbridge Core REST endpoints."
        files = "- `backend/src/api/bridge.rs` \n- `backend/src/services/allbridge.rs`"
        crit = "- Correctly format request and parse Allbridge fee responses."
        guid = "Use `reqwest` client to call `core-api.allbridge.io`."
    elif i == 17:
        title = "Implement bridge transaction status polling backend task"
        desc = "Track cross-chain deposits and poll Allbridge status periodically."
        files = "- `backend/src/api/bridge.rs` \n- `backend/src/services/allbridge.rs`"
        crit = "- Return bridge status (PENDING, SUCCESS, FAILED) dynamically."
        guid = "Expose status endpoint called by the client UI stepper."
    elif i == 18:
        title = "Add structured request logging middleware"
        desc = "Integrate JSON/tracing log details to monitor server requests and errors."
        files = "- `backend/src/main.rs`"
        crit = "- Print status code, path, duration, errors in clean format."
        guid = "Setup `tower_http::trace::TraceLayer`."
    elif i == 19:
        title = "Create rate limiter middleware for sensitive endpoints"
        desc = "Protect login and search endpoints from spam."
        files = "- `backend/src/main.rs`"
        crit = "- Return 429 Too Many Requests if threshold is crossed."
        guid = "Use simple token bucket algorithm using Redis or memory state."
    elif i == 20:
        title = "Write integration tests for feed pagination and privacy logic"
        desc = "Automate verification of public, friends, and private filters."
        files = "- `backend/tests/` module"
        crit = "- Seed test database and run mock Axum requests."
        guid = "Use `axum::serve` in test environment or test endpoints directly."
    else:
        continue
    issues_data.append({"cat": "BE", "num": num, "title": title, "desc": desc, "files": files, "crit": crit, "guid": guid})

# --- MOBILE FRONTEND (FE-001 to FE-015) ---
for i in range(1, 16):
    num = f"{i:03d}"
    if i == 1:
        title = "Implement Social Feed screen tab UI"
        desc = "Redesign Home screen page to host tabs for Public and Friends feeds."
        files = "- `mobileapp/app/(personal)/home.tsx`"
        crit = "- Users can switch tabs.\n- Active tab highlighted in dark green."
        guid = "Use React State to track active tab and filter item lists."
    elif i == 2:
        title = "Design Feed Item component"
        desc = "Create a social transaction card showing payment information in a clean capsule format."
        files = "- `mobileapp/app/(personal)/home.tsx`"
        crit = "- Display avatars, sender/receiver, note, like/comment counts, currency symbol."
        guid = "Align details in columns and render description note in grey background box."
    elif i == 3:
        title = "Integrate Like interaction in Feed Item"
        desc = "Connect like action to toggle heart state locally and call backend API."
        files = "- `mobileapp/app/(personal)/home.tsx`"
        crit = "- Instant UI feedback (scale toggle, count increments).\n- Syncs to DB."
        guid = "Use Animated API to scale heart icon slightly on click."
    elif i == 4:
        title = "Build Comments overlay modal"
        desc = "Design bottom drawer displaying list of transaction comments and comment input."
        files = "- `mobileapp/app/(personal)/home.tsx`"
        crit = "- Drawer opens from bottom.\n- Input sends comments to backend."
        guid = "Use React Native Modal with slide animation."
    elif i == 5:
        title = "Design 'Pay / Request' trigger button"
        desc = "Make social payments easily accessible from the balance card."
        files = "- `mobileapp/app/(personal)/home.tsx`"
        crit = "- Clicking button launches transfer flow."
        guid = "Use TouchableOpacity styled with primary background color."
    elif i == 6:
        title = "Update transfer flow to search recipients"
        desc = "Allow searching users by Zaps ID when making a transfer."
        files = "- `mobileapp/app/transfer.tsx`"
        crit = "- Display matching users in list as user types Zaps ID."
        guid = "Query backend search endpoint and display result list dynamically."
    elif i == 7:
        title = "Implement Note / Description input field in Transfer flow"
        desc = "Add note input with placeholder asking user what the payment is for."
        files = "- `mobileapp/app/transfer.tsx`"
        crit = "- Limit note size to 100 characters max."
        guid = "Configure TextInput with maxLength={100} attribute."
    elif i == 8:
        title = "Implement Payment Visibility Selector dropdown"
        desc = "Add selection to choose Public, Friends, or Private visibility before sending."
        files = "- `mobileapp/app/transfer.tsx`"
        crit = "- Selection changes visibility state and updates transfer summary page."
        guid = "Style button choices with corresponding icons."
    elif i == 9:
        title = "Design custom Naira numeric input keypad"
        desc = "Create a custom numeric keyboard layout matching Naira symbol ₦."
        files = "- `mobileapp/app/transfer.tsx`"
        crit = "- Prevent default device keyboard popup.\n- Render ₦ clearly."
        guid = "Map grid button elements to update amount string state."
    elif i == 10:
        title = "Integrate Stellar wallet connection"
        desc = "Incorporate freighter/albedo signatures or local keys to authorize transfers."
        files = "- `mobileapp/app/transfer.tsx` \n- `mobileapp/src/services/`"
        crit = "- Create and sign transaction envelopes on Stellar."
        guid = "Use `@stellar/stellar-sdk` library to construct transaction envelopes."
    elif i == 11:
        title = "Build Allbridge Bridge Funding screen"
        desc = "Create screen where users select source blockchain and token to bridge."
        files = "- `mobileapp/app/fund.tsx`"
        crit = "- Dropdown selects Solana, Ethereum, etc.\n- Calls backend API for quotes."
        guid = "Bind selectors to fetch quotes on selection changes."
    elif i == 12:
        title = "Design bridge transaction progress screen"
        desc = "Build transaction progress screen showcasing the bridging steps."
        files = "- `mobileapp/app/fund.tsx`"
        crit = "- Step tracker updates dynamically based on polling confirmations."
        guid = "Use ActivityIndicator spinner and custom status stepper."
    elif i == 13:
        title = "Build user profile editing screen"
        desc = "Allow users to customize their display name, bio, and profile avatar."
        files = "- `mobileapp/app/onboarding-start.tsx`"
        crit = "- Save profile data to backend db."
        guid = "Upload files to cloud storage or store URIs directly."
    elif i == 14:
        title = "Add local caching for feed items using AsyncStorage"
        desc = "Cache feed locally to load content instantly on startup."
        files = "- `mobileapp/app/(personal)/home.tsx`"
        crit = "- Load cached items immediately, then fetch updates from backend."
        guid = "Read feed items from AsyncStorage inside useEffect on mount."
    elif i == 15:
        title = "Add haptic feedback to Like/Comment buttons"
        desc = "Integrate responsive vibration haptics on user interactions."
        files = "- `mobileapp/app/(personal)/home.tsx`"
        crit = "- Haptic triggers on clicking like or comment buttons."
        guid = "Use `expo-haptics` library calling `Haptics.impactAsync`."
    else:
        continue
    issues_data.append({"cat": "FE", "num": num, "title": title, "desc": desc, "files": files, "crit": crit, "guid": guid})

# --- WEB DASHBOARD (DB-001 to DB-005) ---
for i in range(1, 6):
    num = f"{i:03d}"
    if i == 1:
        title = "Implement social payments list view under Transactions tab"
        desc = "Create a table showing recent feeds, notes, visibility, and likes."
        files = "- `dashboard/app/dashboard/transactions/page.tsx`"
        crit = "- Table display details.\n- Highlight visibility status."
        guid = "Fetch records from feed backend and display in list."
    elif i == 2:
        title = "Add social metric cards"
        desc = "Display statistics for likes, comments, and active social feeds."
        files = "- `dashboard/app/dashboard/page.tsx`"
        crit = "- Rebrand overview metrics cards."
        guid = "Update overview grid cells to map to backend values."
    elif i == 3:
        title = "Implement active user analytics chart"
        desc = "Show social payment growth trends over time."
        files = "- `dashboard/app/dashboard/analytics/page.tsx`"
        crit = "- Render monthly interaction graphs."
        guid = "Use standard charts component connected to backend query."
    elif i == 4:
        title = "Design Allbridge cross-chain deposit dashboard monitor"
        desc = "Monitor incoming bridge deposits and confirm transaction status."
        files = "- `dashboard/app/dashboard/payouts/page.tsx`"
        crit = "- View logs of bridged transactions."
        guid = "Link query logs to backend bridge logs."
    elif i == 5:
        title = "Build admin system configuration page"
        desc = "Allow admins to adjust naira stablecoin transaction fee settings."
        files = "- `dashboard/app/dashboard/contracts/page.tsx`"
        crit = "- Form updates contract parameter states on-chain."
        guid = "Submit transaction updating fee coefficient inside payment contract."
    else:
        continue
    issues_data.append({"cat": "DB", "num": num, "title": title, "desc": desc, "files": files, "crit": crit, "guid": guid})

# --- DEVOPS & INFRA (DO-001 to DO-005) ---
for i in range(1, 6):
    num = f"{i:03d}"
    if i == 1:
        title = "Setup Docker Compose file for local development"
        desc = "Define services for PostgreSQL database, Redis instance, and Soroban RPC quickstart node."
        files = "- `docker-compose.yml` at project root"
        crit = "- `docker-compose up` launches all 3 backend dependencies."
        guid = "Use official Soroban quickstart images configured for local sandbox."
    elif i == 2:
        title = "Setup Github Actions CI workflow for compiling Soroban smart contracts"
        desc = "Add workflow to compile contracts and verify checks on commits."
        files = "- `.github/workflows/ci-contracts.yml`"
        crit = "- Build wasm files automatically on pull requests."
        guid = "Install Rust target wasm32-unknown-unknown before compiling."
    elif i == 3:
        title = "Setup Github Actions CI workflow for cargo test and clippy on backend"
        desc = "Verify backend code compilation and quality standards on pull request."
        files = "- `.github/workflows/ci-backend.yml`"
        crit = "- Enforce compilation success, formatting checks, and tests passes."
        guid = "Run `cargo fmt --check` and `cargo clippy -- -D warnings`."
    elif i == 4:
        title = "Create Kubernetes deployment manifests for Rust backend API"
        desc = "Prepare configuration files to deploy backend pods and service access layers."
        files = "- `k8s/deployment.yaml`"
        crit = "- Manifest defines deployment, cluster service, configuration mappings."
        guid = "Target docker containers built in CI flows."
    elif i == 5:
        title = "Write Swagger/OpenAPI v3 specification file for backend routes"
        desc = "Document backend API routes to allow frontend integration and testing."
        files = "- `docs/openapi.yaml`"
        crit = "- Details challenge, auth verification, profile, feed, social, and bridge routes."
        guid = "Write accurate path structures matching Axum route inputs/outputs."
    else:
        continue
    issues_data.append({"cat": "DO", "num": num, "title": title, "desc": desc, "files": files, "crit": crit, "guid": guid})

# 1. Write issues locally as markdown
print(f"Writing {len(issues_data)} issue files locally...")
for issue in issues_data:
    cat = issue["cat"]
    subdir = subdirs[cat]
    num = issue["num"]
    title = f"{CATEGORIES[cat].upper()} [{cat}-{num}]: {issue['title']}"
    filename = f"{cat.lower()}_{num}_{issue['title'].lower().replace(' ', '_').replace('/', '_').replace(':', '')[:30]}.md"
    filepath = os.path.join(ISSUES_DIR, subdir, filename)
    
    content = ISSUE_TEMPLATE.format(
        title=title,
        description=issue["desc"],
        files=issue["files"],
        criteria=issue["crit"],
        guid=issue["guid"]
    )
    
    with open(filepath, "w") as f:
        f.write(content)

print(f"Successfully generated {len(issues_data)} local markdown issues inside /issues/ folder.")

# 2. Publish to GitHub
print("Publishing issues to GitHub using 'gh CLI'...")
for idx, issue in enumerate(issues_data):
    cat = issue["cat"]
    num = issue["num"]
    title = f"[{cat}-{num}] {issue['title']}"
    
    # Body text
    body = f"## Description\n{issue['desc']}\n\n## Files to Edit/Create\n{issue['files']}\n\n## Acceptance Criteria\n{issue['crit']}\n\n## Guidance / Hints\n{issue['guid']}"
    
    # Label mapping
    label = CATEGORIES[cat]
    
    print(f"Creating Issue {idx+1}/{len(issues_data)}: {title} ...")
    
    # Run gh issue create command
    cmd = [
        "gh", "issue", "create",
        "--title", title,
        "--body", body,
        "--label", label
    ]
    
    try:
        # Create label first (ignore if it already exists)
        subprocess.run(["gh", "label", "create", label, "--color", "0E4A47"], capture_output=True)
        
        # Create issue
        res = subprocess.run(cmd, capture_output=True, text=True)
        if res.returncode == 0:
            print(f"  Success: {res.stdout.strip()}")
        else:
            print(f"  Error: {res.stderr.strip()}")
    except Exception as e:
        print(f"  Exception occurred: {e}")

print("All 60 issues processed.")
