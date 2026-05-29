import { Request, Response, NextFunction } from 'express';
import metricsService from '../services/metrics.service';
import contractMonitoringService from '../services/contract-monitoring.service';

export const getDashboardStats = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const stats = await metricsService.getDashboardStats();
        res.status(200).json(stats);
    } catch (error) {
        next(error);
    }
};

export const getSystemHealth = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const health = await metricsService.getSystemHealth();
        res.status(200).json(health);
    } catch (error) {
        next(error);
    }
};

export const getContractHealth = async (_req: Request, res: Response, next: NextFunction) => {
    try {
        const health = contractMonitoringService.getHealth();
        res.status(200).json(health);
    } catch (error) {
        next(error);
    }
};

export const getContractMetrics = async (_req: Request, res: Response, next: NextFunction) => {
    try {
        const metrics = contractMonitoringService.getMetrics();
        res.status(200).json(metrics);
    } catch (error) {
        next(error);
    }
};

export const getContractAlerts = async (_req: Request, res: Response, next: NextFunction) => {
    try {
        const alerts = contractMonitoringService.getAlerts();
        res.status(200).json({ alerts });
    } catch (error) {
        next(error);
    }
};

export const getContractPrometheusMetrics = async (_req: Request, res: Response) => {
    res.setHeader('Content-Type', 'text/plain; version=0.0.4; charset=utf-8');
    res.status(200).send(contractMonitoringService.getPrometheusMetrics());
};
