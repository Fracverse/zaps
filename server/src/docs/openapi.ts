/**
 * OpenAPI 3.0 specification for the Zaps (Blink) API.
 *
 * Mount the Swagger UI at /api-docs (see app.ts).
 * The Postman collection can be generated from this spec:
 *   npx @apideck/postman-collection-generator -s src/docs/openapi.ts -o postman.json
 */
export const openApiSpec = {
    openapi: '3.0.3',
    info: {
        title: 'Zaps (Blink) API',
        version: '1.0.0',
        description: `
## Overview
Zaps is a Stellar-powered payments platform supporting merchant QR/NFC payments,
fiat on/off ramps via Stellar anchors (SEP-24/SEP-31), and real-time transaction
monitoring.

## Authentication
All protected endpoints require a **Bearer JWT** in the \`Authorization\` header.
Obtain a token via \`POST /api/v1/auth/login\`.

## Rate Limits
- **Global**: 100 requests / 15 min per IP
- **Write endpoints**: 20 requests / 15 min per IP

## Versioning
The current stable version is **v1** (prefix: \`/api/v1\`).

## Webhooks
Subscribe to transaction events at \`POST /api/v1/webhooks\`.
See the **Webhooks** section below for payload schemas.
        `.trim(),
        contact: { name: 'Zaps Support', email: 'support@zaps.app' },
        license: { name: 'MIT' },
    },
    servers: [
        { url: 'https://api.zaps.app/api/v1', description: 'Production' },
        { url: 'https://testnet.zaps.app/api/v1', description: 'Testnet' },
        { url: 'http://localhost:3001/api/v1', description: 'Local development' },
    ],
    tags: [
        { name: 'Auth', description: 'Authentication and session management' },
        { name: 'Users', description: 'User profile management' },
        { name: 'Payments', description: 'QR/NFC merchant payment initiation' },
        { name: 'Merchants', description: 'Merchant onboarding and management' },
        { name: 'Payouts', description: 'Merchant fiat payout requests' },
        { name: 'Anchor', description: 'Stellar SEP-24/SEP-31 fiat on/off ramps' },
        { name: 'Bridge', description: 'Cross-chain bridge operations' },
        { name: 'Notifications', description: 'In-app notification management' },
        { name: 'Audit', description: 'Audit log access (admin)' },
        { name: 'Health', description: 'Service health checks' },
    ],
    components: {
        securitySchemes: {
            BearerAuth: {
                type: 'http',
                scheme: 'bearer',
                bearerFormat: 'JWT',
                description: 'JWT obtained from POST /auth/login',
            },
        },
        schemas: {
            Error: {
                type: 'object',
                required: ['error', 'code'],
                properties: {
                    error: { type: 'string', example: 'Resource not found' },
                    code: { type: 'string', example: 'NOT_FOUND' },
                    requestId: { type: 'string', example: 'req_abc123' },
                },
            },
            User: {
                type: 'object',
                properties: {
                    id: { type: 'string', format: 'uuid' },
                    email: { type: 'string', format: 'email' },
                    stellarAddress: { type: 'string', example: 'GABC...XYZ' },
                    createdAt: { type: 'string', format: 'date-time' },
                },
            },
            Payment: {
                type: 'object',
                properties: {
                    paymentId: { type: 'string', format: 'uuid' },
                    xdr: { type: 'string', description: 'Unsigned Stellar transaction XDR' },
                    feePayerAddress: { type: 'string' },
                    networkPassphrase: { type: 'string' },
                    status: { type: 'string', enum: ['PENDING', 'CONFIRMED', 'FAILED'] },
                },
            },
            Payout: {
                type: 'object',
                properties: {
                    id: { type: 'string', format: 'uuid' },
                    merchantId: { type: 'string', format: 'uuid' },
                    grossAmount: { type: 'string', description: 'Gross payout in stroops' },
                    feeAmount: { type: 'string', description: 'Platform fee in stroops' },
                    netAmount: { type: 'string', description: 'Net amount in stroops' },
                    asset: { type: 'string', example: 'USDC' },
                    status: {
                        type: 'string',
                        enum: ['PENDING', 'PROCESSING', 'COMPLETED', 'FAILED', 'PERMANENTLY_FAILED'],
                    },
                    createdAt: { type: 'string', format: 'date-time' },
                    completedAt: { type: 'string', format: 'date-time', nullable: true },
                },
            },
        },
        responses: {
            Unauthorized: {
                description: 'Missing or invalid authentication token',
                content: {
                    'application/json': {
                        schema: { $ref: '#/components/schemas/Error' },
                        example: { error: 'Unauthorized', code: 'UNAUTHORIZED' },
                    },
                },
            },
            TooManyRequests: {
                description: 'Rate limit exceeded',
                headers: {
                    'Retry-After': {
                        schema: { type: 'integer' },
                        description: 'Seconds until the rate limit resets',
                    },
                },
                content: {
                    'application/json': {
                        schema: { $ref: '#/components/schemas/Error' },
                        example: { error: 'Too many requests', code: 'RATE_LIMIT_EXCEEDED' },
                    },
                },
            },
        },
    },
    security: [{ BearerAuth: [] }],
    paths: {
        // ── Auth ─────────────────────────────────────────────────────────
        '/auth/register': {
            post: {
                tags: ['Auth'],
                summary: 'Register a new user',
                security: [],
                requestBody: {
                    required: true,
                    content: {
                        'application/json': {
                            schema: {
                                type: 'object',
                                required: ['email', 'password'],
                                properties: {
                                    email: { type: 'string', format: 'email' },
                                    password: { type: 'string', minLength: 8 },
                                    phone: { type: 'string' },
                                },
                            },
                        },
                    },
                },
                responses: {
                    201: { description: 'User registered successfully' },
                    400: { description: 'Validation error' },
                    409: { description: 'Email already registered' },
                },
            },
        },
        '/auth/login': {
            post: {
                tags: ['Auth'],
                summary: 'Login and obtain JWT',
                security: [],
                requestBody: {
                    required: true,
                    content: {
                        'application/json': {
                            schema: {
                                type: 'object',
                                required: ['email', 'password'],
                                properties: {
                                    email: { type: 'string', format: 'email' },
                                    password: { type: 'string' },
                                },
                            },
                        },
                    },
                },
                responses: {
                    200: {
                        description: 'Login successful',
                        content: {
                            'application/json': {
                                schema: {
                                    type: 'object',
                                    properties: {
                                        accessToken: { type: 'string' },
                                        refreshToken: { type: 'string' },
                                        expiresIn: { type: 'integer', example: 3600 },
                                    },
                                },
                            },
                        },
                    },
                    401: { description: 'Invalid credentials' },
                    429: { $ref: '#/components/responses/TooManyRequests' },
                },
            },
        },
        // ── Payments ──────────────────────────────────────────────────────
        '/payments': {
            post: {
                tags: ['Payments'],
                summary: 'Initiate a merchant payment',
                description:
                    'Builds an unsigned, fee-sponsored Soroban transaction XDR. The client ' +
                    'must sign the XDR with the payer wallet and submit it.',
                requestBody: {
                    required: true,
                    content: {
                        'application/json': {
                            schema: {
                                type: 'object',
                                required: ['merchantId', 'amount', 'asset'],
                                properties: {
                                    merchantId: { type: 'string', format: 'uuid' },
                                    amount: { type: 'string', description: 'Amount in stroops' },
                                    asset: { type: 'string', example: 'USDC' },
                                    memo: { type: 'string', maxLength: 28 },
                                },
                            },
                        },
                    },
                },
                responses: {
                    200: {
                        description: 'Unsigned transaction XDR returned',
                        content: {
                            'application/json': {
                                schema: { $ref: '#/components/schemas/Payment' },
                            },
                        },
                    },
                    400: { description: 'Validation error or compliance check failed' },
                    401: { $ref: '#/components/responses/Unauthorized' },
                },
            },
        },
        // ── Payouts ───────────────────────────────────────────────────────
        '/payouts': {
            post: {
                tags: ['Payouts'],
                summary: 'Request a merchant payout',
                description:
                    'Submits a payout request for a verified bank account. Enforces the minimum ' +
                    'threshold and deducts the platform fee. The payout is queued for processing.',
                requestBody: {
                    required: true,
                    content: {
                        'application/json': {
                            schema: {
                                type: 'object',
                                required: ['amount', 'asset', 'bankAccountId', 'anchorId'],
                                properties: {
                                    amount: { type: 'string', description: 'Amount in stroops' },
                                    asset: { type: 'string', example: 'USDC' },
                                    bankAccountId: { type: 'string', format: 'uuid' },
                                    anchorId: { type: 'string', example: 'circle' },
                                },
                            },
                        },
                    },
                },
                responses: {
                    202: {
                        description: 'Payout accepted and queued',
                        content: {
                            'application/json': {
                                schema: { $ref: '#/components/schemas/Payout' },
                            },
                        },
                    },
                    400: { description: 'Below minimum threshold or bank account invalid' },
                    401: { $ref: '#/components/responses/Unauthorized' },
                },
            },
        },
        '/payouts/history': {
            get: {
                tags: ['Payouts'],
                summary: 'Get payout history for authenticated merchant',
                parameters: [
                    { name: 'limit', in: 'query', schema: { type: 'integer', default: 20, maximum: 100 } },
                    { name: 'offset', in: 'query', schema: { type: 'integer', default: 0 } },
                ],
                responses: {
                    200: {
                        description: 'Payout history',
                        content: {
                            'application/json': {
                                schema: {
                                    type: 'object',
                                    properties: {
                                        payouts: {
                                            type: 'array',
                                            items: { $ref: '#/components/schemas/Payout' },
                                        },
                                    },
                                },
                            },
                        },
                    },
                },
            },
        },
        // ── Anchor ────────────────────────────────────────────────────────
        '/anchor/deposit': {
            post: {
                tags: ['Anchor'],
                summary: 'Initiate a SEP-24 fiat deposit',
                description:
                    'Returns an interactive URL that the user opens to complete the deposit ' +
                    'flow at the anchor (KYC, bank transfer, etc.).',
                requestBody: {
                    required: true,
                    content: {
                        'application/json': {
                            schema: {
                                type: 'object',
                                required: ['anchorId', 'assetCode'],
                                properties: {
                                    anchorId: { type: 'string' },
                                    assetCode: { type: 'string', example: 'USDC' },
                                    amountHint: { type: 'string' },
                                },
                            },
                        },
                    },
                },
                responses: {
                    200: {
                        description: 'Interactive URL returned',
                        content: {
                            'application/json': {
                                schema: {
                                    type: 'object',
                                    properties: {
                                        url: { type: 'string', format: 'uri' },
                                        id: { type: 'string' },
                                    },
                                },
                            },
                        },
                    },
                },
            },
        },
        '/anchor/withdraw': {
            post: {
                tags: ['Anchor'],
                summary: 'Initiate a SEP-24 fiat withdrawal',
                responses: { 200: { description: 'Interactive withdrawal URL returned' } },
            },
        },
        '/anchor/transaction/{id}': {
            get: {
                tags: ['Anchor'],
                summary: 'Get SEP-24 anchor transaction status',
                parameters: [{ name: 'id', in: 'path', required: true, schema: { type: 'string' } }],
                responses: { 200: { description: 'Transaction status' } },
            },
        },
        // ── Health ────────────────────────────────────────────────────────
        '/health': {
            get: {
                tags: ['Health'],
                summary: 'Service health check',
                security: [],
                responses: {
                    200: {
                        description: 'Service is healthy',
                        content: {
                            'application/json': {
                                schema: {
                                    type: 'object',
                                    properties: {
                                        status: { type: 'string', example: 'OK' },
                                        timestamp: { type: 'string', format: 'date-time' },
                                    },
                                },
                            },
                        },
                    },
                },
            },
        },
    },
    // ── Webhook payloads ────────────────────────────────────────────────────
    'x-webhooks': {
        'payment.received': {
            post: {
                summary: 'Payment received on Stellar network',
                requestBody: {
                    content: {
                        'application/json': {
                            schema: {
                                type: 'object',
                                properties: {
                                    event: { type: 'string', example: 'payment.received' },
                                    userId: { type: 'string', format: 'uuid' },
                                    amount: { type: 'string' },
                                    asset: { type: 'string' },
                                    txHash: { type: 'string' },
                                    timestamp: { type: 'integer' },
                                },
                            },
                        },
                    },
                },
            },
        },
        'payout.completed': {
            post: {
                summary: 'Merchant payout completed',
                requestBody: {
                    content: {
                        'application/json': {
                            schema: {
                                type: 'object',
                                properties: {
                                    event: { type: 'string', example: 'payout.completed' },
                                    payoutId: { type: 'string', format: 'uuid' },
                                    merchantId: { type: 'string', format: 'uuid' },
                                    netAmount: { type: 'string' },
                                    timestamp: { type: 'integer' },
                                },
                            },
                        },
                    },
                },
            },
        },
    },
};
