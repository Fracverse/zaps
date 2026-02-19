# Zaps - TypeScript Backend (Blueprint)

This is the TypeScript/Express backend for Zaps, designed as a high-fidelity skeletal blueprint. It provides the architectural foundation and implementation guidance for building a non-custodial, Soroban-integrated financial system.

## 🚀 Getting Started

### Prerequisites
- **Node.js**: v18+ 
- **PostgreSQL**: For persistent data storage.
- **Redis**: For rate limiting and background job processing.

### Installation
1.  Navigate to the server directory:
    ```bash
    cd server
    ```
2.  Install dependencies:
    ```bash
    npm install
    ```
    *Note: This will also automatically run `npx prisma generate`.*

### Configuration
1.  Copy the environment template:
    ```bash
    cp .env.example .env
    ```
2.  Configure your variables in `.env`:
    - `DATABASE_URL`: Your PostgreSQL connection string.
    - `REDIS_HOST`/`PORT`: Your Redis connection details.
    - `JWT_SECRET`: A secure key for token signing.
    - `SOROBAN_RPC_URL`: URL for the Stellar/Soroban RPC.

## 🛠️ Development

### Available Scripts
- `npm run dev`: Start the development server with `nodemon` (auto-restarts on change).
- `npm run build`: Compile the TypeScript source code to the `dist` directory.
- `npm start`: Run the compiled production application.
- `npx prisma db push`: Sync your database schema with the Prisma models.

### Directory Structure
- `src/routes/`: API endpoint definitions and routing.
- `src/controllers/`: Request handling and orchestration.
- `src/services/`: Core business logic and external service blueprints.
- `src/middleware/`: Security, Auth, Audit, and Validation logic.
- `src/workers/`: Background job processors (BullMQ).
- `src/utils/`: Shared utilities (Logger, Prisma client, Redis connection).

## 🧩 Blueprint Implementation
This repository follows a **Lean Blueprint** approach:
- **Architectural Skeleton**: The full request flow (`Route -> Controller -> Service`) is implemented.
- **Guidance-Rich**: Comprehensive implementation details for each module can be found in [github-issues.md](./github-issues.md).
- **Mocks & Placeholders**: Highly complex integrations (e.g., S3 storage, actual on-chain transaction submission) use skeletal placeholders. Replace these with your preferred open-source implementations as guided by the roadmap.
