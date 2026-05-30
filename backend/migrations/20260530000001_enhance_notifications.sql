-- Create notification preferences table
CREATE TABLE notification_preferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL UNIQUE,
    email_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    sms_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    push_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create notification templates table
CREATE TABLE notification_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    subject_template TEXT,
    body_template TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create notification delivery logs table
CREATE TYPE delivery_status AS ENUM ('SENT', 'DELIVERED', 'FAILED');
CREATE TYPE delivery_channel AS ENUM ('EMAIL', 'SMS', 'PUSH', 'IN_APP');

CREATE TABLE notification_delivery_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    notification_id UUID NOT NULL REFERENCES notifications(id),
    channel delivery_channel NOT NULL,
    status delivery_status NOT NULL,
    error_message TEXT,
    delivered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_notification_delivery_logs_notification_id ON notification_delivery_logs(notification_id);

-- Seed some default templates
INSERT INTO notification_templates (name, subject_template, body_template)
VALUES 
('payment_received', 'Payment Received', 'You have received a payment of {{amount}} {{asset}} from {{sender}}.'),
('security_alert', 'Security Alert', 'A new login was detected from {{ip_address}} at {{timestamp}}.'),
('system_update', 'System Update', 'Our system will be undergoing maintenance on {{date}}.');
