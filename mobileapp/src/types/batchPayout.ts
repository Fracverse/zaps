export interface BatchPayoutItem {
  id: string;
  recipientName: string;
  recipientAddress: string;
  amount: string;
  currency: string;
  status: 'pending' | 'completed' | 'failed';
}

export interface BatchPayoutSummary {
  id: string;
  totalAmount: string;
  currency: string;
  itemCount: number;
  completedCount: number;
  failedCount: number;
  createdAt: string;
}
