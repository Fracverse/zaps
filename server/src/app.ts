import express, { Express, Request, Response, NextFunction } from 'express';
import cors from 'cors';
import helmet from 'helmet';
import morgan from 'morgan';
import swaggerUi from 'swagger-ui-express';
import routes from './routes';
import { errorHandler } from './middleware/error.middleware';
import { rateLimit } from './middleware/rate-limit.middleware';
import { openApiSpec } from './docs/openapi';

const app: Express = express();

// Middleware
app.use(helmet());
app.use(cors());
app.use(express.json());
app.use(morgan('dev'));
import metricsService from './services/metrics.service';

// ... (existing middleware)
app.use(rateLimit(100));

// Metrics tracking middleware
app.use((req: Request, res: Response, next: NextFunction) => {
    const originalSend = res.send;
    res.send = function (body) {
        metricsService.recordRequest(res.statusCode);
        return originalSend.apply(res, arguments as any);
    };
    next();
});

// Interactive API documentation (Swagger UI) — available at /api-docs
app.use(
    '/api-docs',
    swaggerUi.serve,
    swaggerUi.setup(openApiSpec, {
        customSiteTitle: 'Zaps API Docs',
        swaggerOptions: { persistAuthorization: true },
    }),
);

// Serve the raw OpenAPI spec as JSON for Postman / code generators
app.get('/api-docs.json', (_req: Request, res: Response) => {
    res.json(openApiSpec);
});

// Routes
app.use('/api/v1', routes);

// Health check
app.get('/health', (req: Request, res: Response) => {
    res.status(200).json({ status: 'OK', timestamp: new Date().toISOString() });
});

// Error handling middleware
app.use(errorHandler);

export default app;
