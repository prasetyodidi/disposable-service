// Tipe DTO yang mencerminkan crates/common/src/lib.rs (fitur privacy micro-apps & burn).

export interface ScrubCategoryCount {
  category: string;
  count: number;
}

export interface ScrubReport {
  file_kind: string;
  removed: ScrubCategoryCount[];
}

export interface ClaimTime {
  claim: string;
  raw: unknown;
  iso: string | null;
  valid: boolean;
}

export type TokenStatus = 'active' | 'expired' | 'not_yet_valid';
export type SignatureCheck = 'not_requested' | 'valid' | 'invalid' | 'unsupported';

export interface JwtInspectResult {
  header: unknown;
  payload: unknown;
  header_pretty: string;
  payload_pretty: string;
  times: ClaimTime[];
  status: TokenStatus;
  signature: SignatureCheck;
  warnings: string[];
}

export type BurnCategory = 'sessions' | 'downloads' | 'mcp_sessions' | 'temp_dir';

export interface CategoryStat {
  category: BurnCategory | string;
  entries: number;
  bytes: number;
}

export interface FailedItem {
  category: string;
  reason: string;
}

export interface AuditReport {
  verified: boolean;
  incomplete: boolean;
  before: CategoryStat[];
  after: CategoryStat[];
  total_bytes_before: number;
  remaining_bytes: number;
  remaining_categories: CategoryStat[];
  removed_items: number;
  failed_items: FailedItem[];
  duration_ms: number;
  timestamp_utc: string;
  warnings: string[];
}
