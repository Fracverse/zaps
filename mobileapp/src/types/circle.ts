export type CircleVisibility = "PUBLIC" | "PRIVATE";

export interface CirclePreview {
  id: string;
  name: string;
  description: string;
  objective: string;
  goal_amount: string;
  currency: string;
  progress_amount: string;
  contribution_amount: string;
  contribution_frequency: "DAILY" | "WEEKLY" | "MONTHLY";
  contribution_rules: string;
  membership_restrictions: string;
  visibility: CircleVisibility;
  member_count: number;
  member_limit: number | null;
  is_member: boolean;
  can_join: boolean;
  join_unavailable_reason: string | null;
  owner_username: string;
  created_at: string;
}
