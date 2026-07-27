export interface ZapsUser {
  username: string;
  address: string;
  avatar_url: string | null;
  /** Placeholder — real trust data needs to be wired in by the API layer. */
  isVerified?: boolean;
}
